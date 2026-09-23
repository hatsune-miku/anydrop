//! User-confirmed, single-recipient file offers from clipboard and OS context menus.
use super::{emit_snapshot, ensure_receive_window, send_paths_inner, Backend};
use serde::Serialize;
use std::{collections::HashMap, fs, path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestView {
    id: String,
    clipboard: bool,
    file_name: String,
    bytes: u64,
}
struct Request {
    view: RequestView,
    paths: Vec<String>,
    temporary: Option<tempfile::TempDir>,
}
#[derive(Default)]
pub struct OutgoingState {
    pending: Mutex<Vec<Request>>,
    // A paused/retryable transfer still needs its original, immutable source file.
    sources: Mutex<HashMap<String, tempfile::TempDir>>,
}
impl OutgoingState {
    pub fn finish(&self, key: &str) {
        self.sources.lock().unwrap().remove(key);
    }
}
fn publish(app: &AppHandle) {
    let backend = app.state::<Backend>();
    let _ = app.emit("outgoing-requests", get_outgoing_requests(backend));
    ensure_receive_window(app);
}
fn enqueue(app: &AppHandle, request: Request) -> Result<(), String> {
    let backend = app.state::<Backend>();
    let mut pending = backend.outgoing.pending.lock().unwrap();
    // Only the most recently copied oversized clipboard is pending. Existing file
    // menu requests are kept; no stale confirmation can send replaced contents.
    if request.view.clipboard {
        pending.retain(|r| !r.view.clipboard);
    }
    if pending.len() >= 8 {
        return Err("待确认的发送请求过多，请先处理右侧浮窗".into());
    }
    pending.push(request);
    drop(pending);
    backend.set_status("等待确认文件发送");
    publish(app);
    emit_snapshot(app);
    Ok(())
}
pub fn clipboard(app: &AppHandle, bytes: &[u8], extension: &str) -> Result<(), String> {
    let directory = tempfile::Builder::new()
        .prefix("anydrop-clipboard-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let name = format!("剪贴板-{}.{}", super::now_ms(), extension);
    let path = directory.path().join(&name);
    fs::write(&path, bytes).map_err(|e| format!("无法暂存剪贴板文件：{e}"))?;
    enqueue(
        app,
        Request {
            view: RequestView {
                id: rand::random::<u64>().to_string(),
                clipboard: true,
                file_name: name,
                bytes: bytes.len() as u64,
            },
            paths: vec![path.to_string_lossy().into()],
            temporary: Some(directory),
        },
    )
}
pub fn files(app: &AppHandle, paths: Vec<PathBuf>) -> Result<(), String> {
    if paths.is_empty() {
        return Err("没有选择文件".into());
    }
    let mut bytes = 0;
    let mut canonical = Vec::new();
    for path in &paths {
        let path = path
            .canonicalize()
            .map_err(|e| format!("无法读取 {}：{e}", path.display()))?;
        bytes += path.metadata().map_err(|e| e.to_string())?.len();
        canonical.push(path.to_string_lossy().into());
    }
    let name = paths[0].file_name().unwrap_or_default().to_string_lossy();
    enqueue(
        app,
        Request {
            view: RequestView {
                id: rand::random::<u64>().to_string(),
                clipboard: false,
                file_name: if paths.len() == 1 {
                    name.into()
                } else {
                    format!("{name} 等 {} 项", paths.len())
                },
                bytes,
            },
            paths: canonical,
            temporary: None,
        },
    )
}
#[tauri::command]
pub fn get_outgoing_requests(backend: State<'_, Backend>) -> Vec<RequestView> {
    backend
        .outgoing
        .pending
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.view.clone())
        .collect()
}
#[tauri::command]
pub fn dismiss_outgoing_request(app: AppHandle, backend: State<'_, Backend>, id: String) {
    backend
        .outgoing
        .pending
        .lock()
        .unwrap()
        .retain(|r| r.view.id != id);
    publish(&app);
}
#[tauri::command]
pub async fn confirm_outgoing_request(
    app: AppHandle,
    id: String,
    peer_name: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let backend = app.state::<Backend>();
        let peer = backend
            .peers
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.name == peer_name)
            .cloned()
            .ok_or("所选设备已离线，请重新选择")?;
        let mut pending = backend.outgoing.pending.lock().unwrap();
        let index = pending
            .iter()
            .position(|r| r.view.id == id)
            .ok_or("此发送请求已取消或被新的剪贴板替换")?;
        // Serialize confirmation/cancellation so repeated clicks cannot send twice.
        let (snapshot, key) =
            send_paths_inner(&app, &backend, peer.hosts, pending[index].paths.clone())?;
        let request = pending.remove(index);
        if let Some(directory) = request.temporary {
            // Hold the transfers lock across insertion to avoid a fast terminal
            // callback racing with source ownership handover.
            let transfers = backend.transfers.lock().unwrap();
            if transfers
                .get(&key)
                .is_some_and(|t| !matches!(t.status, 2 | 5 | 7))
            {
                backend
                    .outgoing
                    .sources
                    .lock()
                    .unwrap()
                    .insert(key, directory);
            }
        }
        drop(pending);
        let _ = snapshot;
        publish(&app);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
