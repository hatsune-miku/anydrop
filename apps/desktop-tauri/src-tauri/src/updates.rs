//! Signed Tauri self-updates, independent of LAN transfers and any third-party updater service.
use super::{emit_snapshot, is_terminal_status, start_runtime, stop_runtime, Backend};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Default)]
pub struct UpdateState {
    busy: AtomicBool,
    available: Mutex<Option<Update>>,
    downloaded: Mutex<Option<Vec<u8>>>,
}
struct Busy<'a>(&'a AtomicBool);
impl<'a> Busy<'a> {
    fn acquire(flag: &'a AtomicBool) -> Result<Self, String> {
        flag.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "更新操作正在进行".to_string())?;
        Ok(Self(flag))
    }
}
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    version: String,
    notes: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
}

pub fn configured(app: &AppHandle) -> bool {
    let Some(value) = app.config().plugins.0.get("updater") else {
        return false;
    };
    let key = value.get("pubkey").and_then(|v| v.as_str()).unwrap_or("");
    let endpoints = value.get("endpoints").and_then(|v| v.as_array());
    !key.trim().is_empty()
        && endpoints.is_some_and(|list| {
            !list.is_empty()
                && list
                    .iter()
                    .all(|v| v.as_str().is_some_and(|url| url.starts_with("https://")))
        })
}

#[tauri::command]
pub async fn check_app_update(
    app: AppHandle,
    state: State<'_, UpdateState>,
) -> Result<Option<UpdateInfo>, String> {
    let _busy = Busy::acquire(&state.busy)?;
    if !configured(&app) {
        return Err("此版本尚未配置更新服务".into());
    }
    let mut update = app
        .updater_builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| format!("检查更新失败：{e}"))?;
    if update
        .as_ref()
        .is_some_and(|u| u.download_url.scheme() != "https")
    {
        return Err("更新下载地址必须使用 HTTPS".into());
    }
    if let Some(release) = update.as_mut() {
        // The metadata request is short, but an installer download may use a slow connection.
        release.timeout = Some(std::time::Duration::from_secs(15 * 60));
    }
    let result = update.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        notes: u.body.clone().unwrap_or_default(),
    });
    *state.available.lock().unwrap() = update;
    *state.downloaded.lock().unwrap() = None;
    Ok(result)
}

#[tauri::command]
pub async fn download_app_update(
    app: AppHandle,
    state: State<'_, UpdateState>,
) -> Result<(), String> {
    let _busy = Busy::acquire(&state.busy)?;
    let update = state
        .available
        .lock()
        .unwrap()
        .clone()
        .ok_or("请先检查更新")?;
    let mut downloaded = 0;
    let mut last = std::time::Instant::now();
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                if last.elapsed() >= std::time::Duration::from_millis(100) {
                    let _ = app.emit("app-update-progress", Progress { downloaded, total });
                    last = std::time::Instant::now();
                }
            },
            || {},
        )
        .await
        .map_err(|e| format!("更新下载或签名校验失败：{e}"))?;
    // download() validates the embedded minisign public key before returning bytes.
    let size = bytes.len() as u64;
    let _ = app.emit(
        "app-update-progress",
        Progress {
            downloaded: size,
            total: Some(size),
        },
    );
    *state.downloaded.lock().unwrap() = Some(bytes);
    Ok(())
}

#[tauri::command]
pub async fn install_app_update(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<UpdateState>();
        let _busy = Busy::acquire(&state.busy)?;
        let update = state
            .available
            .lock()
            .unwrap()
            .clone()
            .ok_or("请先检查更新")?;
        let backend = app.state::<Backend>();
        let lifecycle = backend.lifecycle.lock().unwrap();
        if backend
            .transfer_list()
            .iter()
            .any(|t| !is_terminal_status(t.status))
        {
            return Err("请先完成或取消待处理、进行中及已暂停的传输，再安装更新".into());
        }
        let bytes = state
            .downloaded
            .lock()
            .unwrap()
            .take()
            .ok_or("请先下载并校验更新")?;
        let was_running = backend.is_running();
        backend.installing.store(true, Ordering::SeqCst);
        stop_runtime(&backend);
        drop(lifecycle);
        backend.set_status("正在安装更新");
        emit_snapshot(&app);
        if let Err(error) = update.install(&bytes) {
            *state.downloaded.lock().unwrap() = Some(bytes);
            backend.installing.store(false, Ordering::SeqCst);
            let mut message = format!("安装更新失败：{error}");
            if was_running {
                let settings = backend.settings.lock().unwrap().clone();
                if let Err(restore) = start_runtime(&app, &backend, settings) {
                    message.push_str(&format!("；服务恢复失败：{restore}"));
                }
            }
            backend.set_status(&message);
            emit_snapshot(&app);
            return Err(message);
        }
        // Windows NSIS may exit during install; macOS/Linux reach this restart.
        app.restart();
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn operation_guard_releases_after_error() {
        let flag = AtomicBool::new(false);
        let guard = Busy::acquire(&flag).unwrap();
        assert!(Busy::acquire(&flag).is_err());
        drop(guard);
        assert!(Busy::acquire(&flag).is_ok());
    }
}
