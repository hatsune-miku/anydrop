use super::*;
use rmcp::{
    model::{CallToolRequestParams, ClientConfig},
    transport::{
        streamable_http_client::StreamableHttpClientTransportConfig, StreamableHttpClientTransport,
    },
    ServiceExt,
};
use std::sync::atomic::{AtomicUsize, Ordering};

fn http_client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

struct Fixture {
    events: Arc<events::Events>,
    active: Arc<AtomicUsize>,
}
impl Operations for Fixture {
    fn version(&self) -> String {
        "test".into()
    }
    fn tools(&self) -> Vec<Tool> {
        actions::tools()
    }
    fn call(&self, name: String, arguments: Value, cancel: CancellationToken) -> Call {
        let events = self.events.clone();
        let active = self.active.clone();
        Box::pin(async move {
            struct Active(Arc<AtomicUsize>);
            impl Drop for Active {
                fn drop(&mut self) {
                    self.0.fetch_sub(1, Ordering::SeqCst);
                }
            }
            active.fetch_add(1, Ordering::SeqCst);
            let _active = Active(active);
            match name.as_str() {
                "get_snapshot" => Ok(json!({"running":true,"fixture":true})),
                "get_event_cursor" => Ok(json!({"cursor":events.cursor()})),
                "wait_for_clipboard_text" | "wait_for_file" => {
                    let options = serde_json::from_value(arguments)
                        .map_err(|e| ApiError::invalid(format!("{e}")))?;
                    let kind = if name == "wait_for_file" {
                        events::Kind::FileReceived
                    } else {
                        events::Kind::ClipboardText
                    };
                    serde_json::to_value(events.wait(kind, options, cancel).await?)
                        .map_err(|e| ApiError::internal(e.to_string()))
                }
                _ => Err(ApiError::not_found(&name)),
            }
        })
    }
}

struct Server {
    url: String,
    fixture: Arc<Fixture>,
    stop: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.cancel();
        self.task.abort();
    }
}
async fn server() -> Server {
    anydrop::transfer::cert::ensure_crypto_installed();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let fixture = Arc::new(Fixture {
        events: Arc::new(events::Events::default()),
        active: Arc::new(AtomicUsize::new(0)),
    });
    let stop = CancellationToken::new();
    let router = router(fixture.clone(), address.port(), stop.clone());
    let shutdown = stop.clone();
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap();
    });
    Server {
        url: format!("http://{address}"),
        fixture,
        stop,
        task,
    }
}

async fn active_count(server: &Server, expected: usize) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while server.fixture.active.load(Ordering::SeqCst) != expected {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("active request count");
}

#[tokio::test]
async fn official_mcp_client_and_http_share_catalogue_and_results() {
    let server = server().await;
    let http = http_client();
    let inventory: Value = http
        .get(format!("{}/api", server.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let transport = StreamableHttpClientTransport::with_client(
        http_client(),
        StreamableHttpClientTransportConfig::with_uri(format!("{}/mcp", server.url)),
    );
    let client = tokio::time::timeout(
        Duration::from_secs(5),
        ClientConfig::default().serve(transport),
    )
    .await
    .unwrap()
    .unwrap();
    let tools = client.list_all_tools().await.unwrap();
    let names: Vec<_> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    let http_names: Vec<_> = inventory["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, http_names);
    assert!(names.contains(&"wait_for_file") && names.contains(&"wait_for_clipboard_text"));
    let mcp = client
        .call_tool(CallToolRequestParams::new("get_snapshot"))
        .await
        .unwrap();
    let rest: Value = http
        .post(format!("{}/api/get_snapshot", server.url))
        .json(&json!({}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(mcp.structured_content.unwrap(), rest);
    let rpc: Value = http
        .post(format!("{}/api", server.url))
        .json(&json!({"method":"get_snapshot"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(rpc, rest);
    let schema: Value = http
        .get(format!("{}/api/openapi.json", server.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(schema["paths"].as_object().unwrap().len(), names.len());
    fn verify_references(value: &Value, document: &Value) {
        match value {
            Value::Object(object) => {
                if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                    let pointer = reference.strip_prefix('#').expect("local schema reference");
                    assert!(
                        document.pointer(pointer).is_some(),
                        "Unresolved schema reference: {reference}"
                    );
                }
                for child in object.values() {
                    verify_references(child, document);
                }
            }
            Value::Array(array) => {
                for child in array {
                    verify_references(child, document);
                }
            }
            _ => (),
        }
    }
    verify_references(&schema, &schema);
    assert_eq!(
        schema["components"]["schemas"]["save_settings"]["properties"]["settings"]["$ref"],
        "#/components/schemas/save_settings/$defs/SettingsChange"
    );
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn legacy_streamable_http_initialize_list_and_call_work() {
    let server = server().await;
    let http = http_client();
    for version in ["2025-03-26", "2025-11-25"] {
        let url = format!("{}/mcp", server.url);
        let initialized: Value = http.post(&url).header("Accept", "application/json, text/event-stream")
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"legacy-test","version":"1"}}}))
            .send().await.unwrap().json().await.unwrap();
        assert_eq!(
            initialized["result"]["protocolVersion"], version,
            "{initialized}"
        );
        let notice = http
            .post(&url)
            .header("MCP-Protocol-Version", version)
            .header("Accept", "application/json, text/event-stream")
            .json(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .send()
            .await
            .unwrap();
        assert!(notice.status().is_success());
        let result: Value = http.post(&url).header("MCP-Protocol-Version", version).header("Accept", "application/json, text/event-stream")
            .json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_snapshot","arguments":{}}}))
            .send().await.unwrap().json().await.unwrap();
        assert_eq!(
            result["result"]["structuredContent"]["result"]["fixture"], true,
            "{result}"
        );
    }
}

#[tokio::test]
async fn waits_do_not_block_other_commands_and_survive_cursor_retries() {
    let server = server().await;
    let http = http_client();
    let cursor = server.fixture.events.cursor();
    let first = tokio::spawn({
        let http = http.clone();
        let url = server.url.clone();
        let cursor = cursor.clone();
        async move {
            http.post(format!("{url}/api/wait_for_clipboard_text"))
                .json(&json!({"after":cursor,"timeoutMs":3000}))
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap()
        }
    });
    let second = tokio::spawn({
        let http = http.clone();
        let url = server.url.clone();
        let cursor = cursor.clone();
        async move {
            http.post(format!("{url}/api/wait_for_clipboard_text"))
                .json(&json!({"after":cursor,"timeoutMs":3000}))
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap()
        }
    });
    active_count(&server, 2).await;
    let snapshot = tokio::time::timeout(
        Duration::from_secs(2),
        http.post(format!("{}/api/get_snapshot", server.url))
            .json(&json!({}))
            .send(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(snapshot.status().is_success());
    server.fixture.events.publish(
        events::Kind::ClipboardText,
        json!({"text":"全文\n第二行","peer":"peer@127.0.0.1"}),
    );
    let (first, second) = (first.await.unwrap(), second.await.unwrap());
    assert_eq!(first, second);
    assert_eq!(first["result"]["event"]["data"]["text"], "全文\n第二行");
    let result: Value = http
        .post(format!("{}/api/wait_for_file", server.url))
        .json(&json!({"after":cursor,"timeoutMs":1}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(result["result"]["timedOut"], true);
    server.fixture.events.publish(events::Kind::FileReceived, json!({"transferKey":"9","localPath":"/files/中文.txt","paths":["/files/中文.txt","/files/folder"]}));
    let result: Value = http
        .post(format!("{}/api/wait_for_file", server.url))
        .json(&json!({"after":result["result"]["cursor"],"timeoutMs":1000}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        result["result"]["event"]["data"]["paths"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn loopback_api_rejects_browser_origins_rebinding_and_non_json_writes() {
    let server = server().await;
    let http = http_client();
    for path in ["/api", "/mcp"] {
        let forbidden = http
            .post(format!("{}{path}", server.url))
            .header("Origin", "https://example.com")
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
        let rebinding = http
            .post(format!("{}{path}", server.url))
            .header("Host", "evil.example:26937")
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(rebinding.status(), StatusCode::FORBIDDEN);
    }
    let form = http
        .post(format!("{}/api/quit_app", server.url))
        .header("Content-Type", "text/plain")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(form.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let unknown = http
        .post(format!("{}/api/missing", server.url))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
    let malformed = http
        .post(format!("{}/api", server.url))
        .header("Content-Type", "application/json")
        .body("{")
        .send()
        .await
        .unwrap();
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn disconnected_http_wait_releases_its_request() {
    let server = server().await;
    let task = tokio::spawn({
        let url = server.url.clone();
        async move {
            reqwest::Client::builder()
                .no_proxy()
                .pool_max_idle_per_host(0)
                .build()
                .unwrap()
                .post(format!("{url}/api/wait_for_clipboard_text"))
                .json(&json!({"timeoutMs":0}))
                .send()
                .await
        }
    });
    active_count(&server, 1).await;
    task.abort();
    let _ = task.await;
    active_count(&server, 0).await;
}

#[tokio::test]
async fn cancelled_mcp_wait_releases_its_request() {
    let server = server().await;
    let transport = StreamableHttpClientTransport::with_client(
        http_client(),
        StreamableHttpClientTransportConfig::with_uri(format!("{}/mcp", server.url)),
    );
    let client = ClientConfig::default().serve(transport).await.unwrap();
    let peer = client.peer().clone();
    let request = tokio::spawn(async move {
        peer.call_tool(
            CallToolRequestParams::new("wait_for_clipboard_text")
                .with_arguments(json!({"timeoutMs":0}).as_object().unwrap().clone()),
        )
        .await
    });
    active_count(&server, 1).await;
    client.cancel().await.unwrap();
    let _ = request.await;
    active_count(&server, 0).await;
}
