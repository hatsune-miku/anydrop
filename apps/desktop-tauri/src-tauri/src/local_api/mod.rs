//! Loopback-only control surfaces backed by the same desktop operations.
mod actions;
pub mod events;

use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rmcp::{
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ErrorCode, Implementation,
        ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext,
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ErrorData, RoleServer, ServerHandler,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;

const PORT: u16 = 26937;
const MAX_BODY: usize = 32 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct ApiError {
    code: &'static str,
    message: String,
}
impl ApiError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_arguments",
            message: message.into(),
        }
    }
    fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internal_error",
            message: message.into(),
        }
    }
    fn not_found(name: &str) -> Self {
        Self {
            code: "unknown_operation",
            message: format!("Unknown operation: {name}"),
        }
    }
    fn status(&self) -> StatusCode {
        match self.code {
            "invalid_arguments" => StatusCode::BAD_REQUEST,
            "unknown_operation" => StatusCode::NOT_FOUND,
            "internal_error" => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::CONFLICT,
        }
    }
}
impl From<String> for ApiError {
    fn from(message: String) -> Self {
        Self {
            code: "operation_failed",
            message,
        }
    }
}

type Call = Pin<Box<dyn Future<Output = Result<Value, ApiError>> + Send>>;
trait Operations: Send + Sync + 'static {
    fn version(&self) -> String;
    fn tools(&self) -> Vec<Tool>;
    fn call(&self, name: String, arguments: Value, cancel: CancellationToken) -> Call;
}

struct Desktop(AppHandle);
impl Operations for Desktop {
    fn version(&self) -> String {
        self.0.package_info().version.to_string()
    }
    fn tools(&self) -> Vec<Tool> {
        actions::tools()
    }
    fn call(&self, name: String, arguments: Value, cancel: CancellationToken) -> Call {
        Box::pin(actions::call(self.0.clone(), name, arguments, cancel))
    }
}

#[derive(Clone)]
struct Mcp(Arc<dyn Operations>);
impl ServerHandler for Mcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("anydrop", self.0.version()))
            .with_instructions("Control the running AnyDrop desktop app. Read get_snapshot for peer hosts and transfer keys. wait_for_file defaults to completed reception and returns local paths; it does not auto-accept offers. Wait results include cursors for lossless retry within the current app session.")
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.0.tools().into_iter().find(|tool| tool.name == name)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.0.tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if self.get_tool(&request.name).is_none() {
            return Err(ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                format!("Unknown operation: {}", request.name),
                None,
            ));
        }
        let arguments = Value::Object(request.arguments.unwrap_or_default());
        Ok(match self
            .0
            .call(request.name.into_owned(), arguments, context.ct)
            .await
        {
            Ok(result) => CallToolResult::structured(json!({"ok": true, "result": result})),
            Err(error) => CallToolResult::structured_error(json!({"ok":false,"error":error})),
        }
        .into())
    }
}

fn response(result: Result<Value, ApiError>) -> Response {
    match result {
        Ok(result) => Json(json!({"ok":true,"result":result})).into_response(),
        Err(error) => (error.status(), Json(json!({"ok":false,"error":error}))).into_response(),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApiRequest {
    method: String,
    #[serde(default = "empty_arguments")]
    params: Value,
}
fn empty_arguments() -> Value {
    json!({})
}

async fn api_call(
    State(operations): State<Arc<dyn Operations>>,
    Path(name): Path<String>,
    Json(arguments): Json<Value>,
) -> Response {
    response(
        operations
            .call(name, arguments, CancellationToken::new())
            .await,
    )
}
async fn api_rpc(
    State(operations): State<Arc<dyn Operations>>,
    Json(request): Json<ApiRequest>,
) -> Response {
    response(
        operations
            .call(request.method, request.params, CancellationToken::new())
            .await,
    )
}
async fn catalogue(State(operations): State<Arc<dyn Operations>>) -> Json<Value> {
    Json(
        json!({"name":"AnyDrop", "version":operations.version(), "mcp":"/mcp", "api":"/api", "operations":operations.tools()}),
    )
}
async fn openapi(State(operations): State<Arc<dyn Operations>>) -> Json<Value> {
    let mut paths = serde_json::Map::new();
    for tool in operations.tools() {
        paths.insert(format!("/api/{}", tool.name), json!({"post": {
            "operationId":tool.name, "description":tool.description,
            "requestBody":{"required":true,"content":{"application/json":{"schema":tool.input_schema}}},
            "responses":{"200":{"description":"Operation result: {ok:true,result:...}"},
                "400":{"description":"Invalid arguments"},"409":{"description":"Operation could not complete: {ok:false,error:{code,message}}"}}
        }}));
    }
    Json(
        json!({"openapi":"3.1.0", "info":{"title":"AnyDrop local API","version":operations.version()}, "paths":paths}),
    )
}

fn allowed_headers(headers: &HeaderMap, port: u16) -> bool {
    let hosts = [
        format!("localhost:{port}"),
        format!("127.0.0.1:{port}"),
        format!("[::1]:{port}"),
    ];
    let host_allowed = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|host| {
            hosts
                .iter()
                .any(|allowed| host.eq_ignore_ascii_case(allowed))
        });
    host_allowed
        && headers.get("origin").is_none_or(|origin| {
            origin.to_str().ok().is_some_and(|origin| {
                hosts
                    .iter()
                    .any(|host| origin.eq_ignore_ascii_case(&format!("http://{host}")))
            })
        })
}
async fn loopback_guard(
    State(port): State<u16>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if !allowed_headers(request.headers(), port) {
        return (StatusCode::FORBIDDEN, Json(json!({"ok":false,"error":{"code":"forbidden_origin","message":"Only local clients may call this endpoint"}}))).into_response();
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("cache-control", "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert("x-content-type-options", "nosniff".parse().unwrap());
    response
}

fn router(operations: Arc<dyn Operations>, port: u16, cancellation: CancellationToken) -> Router {
    let mcp = Mcp(operations.clone());
    let mut config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_allowed_hosts([
            format!("localhost:{port}"),
            format!("127.0.0.1:{port}"),
            format!("[::1]:{port}"),
        ])
        .with_allowed_origins([
            format!("http://localhost:{port}"),
            format!("http://127.0.0.1:{port}"),
            format!("http://[::1]:{port}"),
        ])
        .with_cancellation_token(cancellation);
    config.max_request_body_bytes = MAX_BODY;
    let service: StreamableHttpService<Mcp, LocalSessionManager> =
        StreamableHttpService::new(move || Ok(mcp.clone()), Default::default(), config);
    Router::new()
        .route("/api", get(catalogue).post(api_rpc))
        .route("/api/openapi.json", get(openapi))
        .route("/api/{operation}", post(api_call))
        .with_state(operations)
        .nest_service("/mcp", service)
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(middleware::from_fn_with_state(port, loopback_guard))
}

#[derive(Default)]
pub struct ServerState {
    shutdown: CancellationToken,
}

pub fn port() -> u16 {
    #[cfg(debug_assertions)]
    if std::env::var_os("ANYDROP_TEST_CONFIG_DIR").is_some() {
        if let Ok(value) = std::env::var("ANYDROP_TEST_API_PORT") {
            if let Ok(port) = value.parse::<u16>() {
                if port != 0 {
                    return port;
                }
            }
        }
    }
    PORT
}

pub fn stop(app: &AppHandle) {
    if let Some(state) = app.try_state::<ServerState>() {
        state.shutdown.cancel();
    }
}

pub fn start(app: AppHandle) {
    let shutdown = app.state::<ServerState>().shutdown.clone();
    tauri::async_runtime::spawn(async move {
        let port = port();
        let mut last_error = None;
        loop {
            let listeners = async {
                let v4 =
                    tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
                let mut listeners = vec![v4];
                match tokio::net::TcpListener::bind((std::net::Ipv6Addr::LOCALHOST, port)).await {
                    Ok(v6) => listeners.push(v6),
                    Err(error)
                        if error.kind() == std::io::ErrorKind::AddrNotAvailable
                            || error.kind() == std::io::ErrorKind::Unsupported =>
                    {
                        ()
                    }
                    Err(error) => return Err(error),
                }
                Ok::<_, std::io::Error>(listeners)
            }
            .await;
            match listeners {
                Ok(listeners) => {
                    last_error = None;
                    app.state::<crate::Backend>()
                        .log(format!("Local MCP/API listening on localhost:{port}"));
                    let running = shutdown.child_token();
                    let router = router(Arc::new(Desktop(app.clone())), port, running.clone());
                    let mut servers = tokio::task::JoinSet::new();
                    for listener in listeners {
                        let router = router.clone();
                        let stop = running.clone();
                        servers.spawn(async move {
                            axum::serve(listener, router)
                                .with_graceful_shutdown(stop.cancelled_owned())
                                .await
                        });
                    }
                    tokio::select! {
                        _ = shutdown.cancelled() => { running.cancel(); servers.abort_all(); return; }
                        result = servers.join_next() => {
                            running.cancel();
                            servers.abort_all();
                            app.state::<crate::Backend>().log(format!("Local MCP/API listener stopped: {result:?}"));
                        }
                    }
                }
                Err(error) => {
                    let message = format!("无法启动本机接口 localhost:{port}：{error}");
                    if last_error.as_ref() != Some(&message) {
                        app.state::<crate::Backend>().log(&message);
                        let _ = app.emit("desktop-notice", &message);
                        crate::emit_snapshot(&app);
                    }
                    last_error = Some(message);
                }
            }
            tokio::select! { _ = shutdown.cancelled() => return, _ = tokio::time::sleep(Duration::from_secs(5)) => () }
        }
    });
}

#[cfg(test)]
mod tests;
