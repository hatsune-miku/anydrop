//! Desktop integrations. Native registrations survive a hidden/unloaded main webview.
use super::{config_base, emit_snapshot, send_current_clipboard, Backend};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_global_shortcut::{
    GlobalShortcutExt, Modifiers, Shortcut, ShortcutEvent, ShortcutState,
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
struct SavedPreferences {
    clipboard_shortcut: String,
    auto_receive_files: bool,
    file_context_menu_enabled: bool,
}
impl Default for SavedPreferences {
    fn default() -> Self {
        Self {
            clipboard_shortcut: String::new(),
            auto_receive_files: false,
            file_context_menu_enabled: true,
        }
    }
}

#[derive(Default)]
struct Binding {
    configured: String,
    active: Option<Shortcut>,
    error: Option<String>,
}

#[derive(Default)]
pub struct DesktopState {
    preferences: Mutex<SavedPreferences>,
    context_menu_error: Mutex<Option<String>>,
    binding: Mutex<Binding>,
    // Serialize changes without holding the binding lock while dispatching to the main thread.
    change: Mutex<()>,
    pressed: AtomicBool,
    recording: AtomicBool,
    sending: AtomicBool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPreferences {
    autostart: bool,
    clipboard_shortcut: String,
    shortcut_registered: bool,
    shortcut_error: Option<String>,
    updater_configured: bool,
    auto_receive_files: bool,
    file_context_menu_enabled: bool,
    file_context_menu_error: Option<String>,
}

pub fn parse_shortcut(value: &str) -> Result<Option<Shortcut>, String> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    let shortcut: Shortcut = value
        .trim()
        .parse()
        .map_err(|_| "无法识别此快捷键".to_string())?;
    if !shortcut
        .mods
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER)
    {
        return Err("快捷键需要包含 Ctrl、Alt 或 Command / Win".into());
    }
    Ok(Some(shortcut))
}

fn save_preferences(saved: &SavedPreferences) -> Result<(), String> {
    let base = config_base();
    fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(saved).map_err(|e| e.to_string())?;
    let temp = base.join("desktop-settings.json.tmp");
    fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    fs::rename(&temp, base.join("desktop-settings.json")).map_err(|e| e.to_string())
}

pub fn auto_receive_files(app: &AppHandle) -> bool {
    app.state::<DesktopState>()
        .preferences
        .lock()
        .unwrap()
        .auto_receive_files
}

#[tauri::command]
pub async fn set_auto_receive_files(
    app: AppHandle,
    enabled: bool,
) -> Result<DesktopPreferences, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<DesktopState>();
        let _change = state.change.lock().unwrap();
        let mut next = state.preferences.lock().unwrap().clone();
        next.auto_receive_files = enabled;
        save_preferences(&next)?;
        *state.preferences.lock().unwrap() = next;
        get_desktop_preferences(app.clone(), app.state::<DesktopState>())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn set_file_context_menu(
    app: AppHandle,
    enabled: bool,
) -> Result<DesktopPreferences, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<DesktopState>();
        let _change = state.change.lock().unwrap();
        let mut next = state.preferences.lock().unwrap().clone();
        next.file_context_menu_enabled = enabled;
        // Preserve the requested preference even if the OS registration needs
        // repair. The error is visible beside the repair button.
        save_preferences(&next)?;
        *state.preferences.lock().unwrap() = next;
        *state.context_menu_error.lock().unwrap() =
            super::context_menu::register(&app, enabled).err();
        get_desktop_preferences(app.clone(), app.state::<DesktopState>())
    })
    .await
    .map_err(|e| e.to_string())?
}

pub fn initialize(app: AppHandle) {
    // Plugin registration dispatches to the main thread and waits; never call it from setup itself.
    std::thread::spawn(move || {
        let state = app.state::<DesktopState>();
        let _change = state.change.lock().unwrap();
        let result = (|| {
            let raw = match fs::read(config_base().join("desktop-settings.json")) {
                Ok(raw) => raw,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => b"{}".to_vec(),
                Err(e) => return Err(e.to_string()),
            };
            let saved: SavedPreferences =
                serde_json::from_slice(&raw).map_err(|e| format!("桌面设置无法读取：{e}"))?;
            *state.preferences.lock().unwrap() = saved.clone();
            *state.context_menu_error.lock().unwrap() =
                super::context_menu::register(&app, saved.file_context_menu_enabled).err();
            state.binding.lock().unwrap().configured = saved.clipboard_shortcut.clone();
            if let Some(shortcut) = parse_shortcut(&saved.clipboard_shortcut)? {
                app.global_shortcut()
                    .register(shortcut)
                    .map_err(|e| format!("快捷键注册失败，可能已被占用：{e}"))?;
                state.binding.lock().unwrap().active = Some(shortcut);
            }
            Ok::<(), String>(())
        })();
        if let Err(error) = result {
            state.binding.lock().unwrap().error = Some(error.clone());
            app.state::<Backend>().log(error);
        }
        let _ = app.emit("desktop-preferences-changed", ());
    });
}

pub fn on_shortcut(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    let state = app.state::<DesktopState>();
    if state.recording.load(Ordering::SeqCst) {
        state.pressed.store(false, Ordering::SeqCst);
        return;
    }
    if state
        .binding
        .lock()
        .unwrap()
        .active
        .as_ref()
        .map(Shortcut::id)
        != Some(shortcut.id())
    {
        return;
    }
    if event.state == ShortcutState::Released {
        state.pressed.store(false, Ordering::SeqCst);
        return;
    }
    // Ignore OS key repeat and overlapping sends. A fresh press requires a release.
    if state.pressed.swap(true, Ordering::SeqCst) || state.sending.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let backend = app.state::<Backend>();
        let result = send_current_clipboard(&app, &backend);
        let message = match result {
            Ok(true) => "已发送当前剪贴板到局域网设备".to_string(),
            Ok(false) => "等待确认以文件形式发送".to_string(),
            Err(error) => format!("快捷键发送失败：{error}"),
        };
        backend.log(&message);
        backend.set_status(&message);
        let _ = app.emit("desktop-notice", &message);
        emit_snapshot(&app);
        app.state::<DesktopState>()
            .sending
            .store(false, Ordering::SeqCst);
    });
}

#[tauri::command]
pub fn get_desktop_preferences(
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<DesktopPreferences, String> {
    let binding = state.binding.lock().unwrap();
    let saved = state.preferences.lock().unwrap();
    Ok(DesktopPreferences {
        autostart: app.autolaunch().is_enabled().map_err(|e| e.to_string())?,
        clipboard_shortcut: binding.configured.clone(),
        shortcut_registered: binding.active.is_some(),
        shortcut_error: binding.error.clone(),
        updater_configured: super::updates::configured(&app),
        auto_receive_files: saved.auto_receive_files,
        file_context_menu_enabled: saved.file_context_menu_enabled,
        file_context_menu_error: state.context_menu_error.lock().unwrap().clone(),
    })
}

#[tauri::command]
pub async fn set_autostart(app: AppHandle, enabled: bool) -> Result<DesktopPreferences, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if enabled {
            app.autolaunch().enable()
        } else {
            app.autolaunch().disable()
        }
        .map_err(|e| e.to_string())?;
        get_desktop_preferences(app.clone(), app.state::<DesktopState>())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn set_clipboard_shortcut(
    app: AppHandle,
    shortcut: String,
) -> Result<DesktopPreferences, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<DesktopState>();
        let _change = state.change.lock().unwrap();
        let next = parse_shortcut(&shortcut)?;
        let old = state.binding.lock().unwrap().active;
        let normalized = next.map(|key| key.to_string()).unwrap_or_default();
        if next != old {
            // Register first; if another app owns the new key the previous binding remains intact.
            if let Some(key) = next {
                app.global_shortcut()
                    .register(key)
                    .map_err(|e| format!("快捷键注册失败，可能已被占用：{e}"))?;
            }
            if let Some(key) = old {
                if let Err(e) = app.global_shortcut().unregister(key) {
                    if let Some(key) = next {
                        let _ = app.global_shortcut().unregister(key);
                    }
                    return Err(format!("无法释放旧快捷键：{e}"));
                }
            }
        }
        let mut next_preferences = state.preferences.lock().unwrap().clone();
        next_preferences.clipboard_shortcut = normalized.clone();
        if let Err(error) = save_preferences(&next_preferences) {
            if next != old {
                if let Some(key) = next {
                    let _ = app.global_shortcut().unregister(key);
                }
                if let Some(key) = old {
                    if let Err(restore_error) = app.global_shortcut().register(key) {
                        let mut binding = state.binding.lock().unwrap();
                        binding.active = None;
                        binding.error =
                            Some(format!("保存失败且旧快捷键恢复失败：{restore_error}"));
                    }
                }
            }
            return Err(format!("快捷键未保存：{error}"));
        }
        *state.preferences.lock().unwrap() = next_preferences;
        *state.binding.lock().unwrap() = Binding {
            configured: normalized,
            active: next,
            error: None,
        };
        state.pressed.store(false, Ordering::SeqCst);
        drop(_change);
        get_desktop_preferences(app.clone(), app.state::<DesktopState>())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_disables_and_unmodified_keys_are_rejected() {
        assert!(parse_shortcut(" ").unwrap().is_none());
        assert!(parse_shortcut("KeyV").is_err());
        assert!(parse_shortcut("Shift+KeyV").is_err());
        assert!(parse_shortcut("Control+Shift+KeyV").unwrap().is_some());
        assert!(parse_shortcut("not-a-shortcut").is_err());
    }
    #[test]
    fn legacy_missing_preferences_are_optional() {
        let defaults = SavedPreferences::default();
        assert!(!defaults.auto_receive_files);
        assert!(defaults.file_context_menu_enabled);
        assert!(serde_json::from_str::<SavedPreferences>("{}")
            .unwrap()
            .clipboard_shortcut
            .is_empty());
    }
}

#[tauri::command]
pub fn set_shortcut_recording(state: State<'_, DesktopState>, recording: bool) {
    state.recording.store(recording, Ordering::SeqCst);
    state.pressed.store(false, Ordering::SeqCst);
}

pub fn end_recording(app: &AppHandle) {
    if let Some(state) = app.try_state::<DesktopState>() {
        state.recording.store(false, Ordering::SeqCst);
    }
}
