//! One operation catalogue and dispatcher for both local transports.
use super::{
    events::{Kind, WaitOptions},
    ApiError,
};
use crate::{desktop, outgoing, receipts, updates, AppSettings, Backend};
use rmcp::model::{Tool, ToolAnnotations};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Empty {}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TransferKey {
    transfer_key: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AcceptTransfer {
    transfer_key: String,
    /// Absolute destination directory; omitted uses the GUI's default directory.
    save_dir: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SendText {
    hosts: Vec<String>,
    text: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SendPaths {
    /// Addresses of one peer, obtained from get_snapshot.
    hosts: Vec<String>,
    /// Absolute local file or directory paths.
    paths: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Text {
    text: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Path {
    path: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Enabled {
    enabled: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Shortcut {
    shortcut: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PeerRemark {
    peer_key: String,
    remark: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RequestId {
    id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfirmRequest {
    id: String,
    peer_name: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SettingsChange {
    send_clipboard_enabled: Option<bool>,
    receive_clipboard_enabled: Option<bool>,
    send_only_on_double_copy: Option<bool>,
    group_identity: Option<u32>,
    discovery_port: Option<u16>,
    data_port: Option<u16>,
    display_name: Option<String>,
    sync_image_enabled: Option<bool>,
    /// true = dark, false = light, null = follow the system.
    #[serde(default, deserialize_with = "present_nullable")]
    dark_mode: Option<Option<bool>>,
    default_save_dir: Option<String>,
    clipboard_popup_enabled: Option<bool>,
    suppress_popup_in_game: Option<bool>,
}

fn present_nullable<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Option<bool>>, D::Error> {
    Option::<bool>::deserialize(d).map(Some)
}

impl SettingsChange {
    fn apply(self, current: &mut AppSettings) -> Result<(), ApiError> {
        if self.discovery_port == Some(0) || self.data_port == Some(0) {
            return Err(ApiError::invalid("Ports must be between 1 and 65535"));
        }
        if let Some(path) = &self.default_save_dir {
            absolute(path)?;
        }
        macro_rules! update { ($($field:ident),*) => { $(if let Some(value) = self.$field { current.$field = value; })* }; }
        update!(
            send_clipboard_enabled,
            receive_clipboard_enabled,
            send_only_on_double_copy,
            group_identity,
            discovery_port,
            data_port,
            display_name,
            sync_image_enabled,
            dark_mode,
            default_save_dir,
            clipboard_popup_enabled,
            suppress_popup_in_game
        );
        Ok(())
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SaveSettings {
    settings: SettingsChange,
}

#[derive(Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum FileStage {
    Offered,
    #[default]
    Completed,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WaitFile {
    /// completed (default) waits for all files to be saved; offered waits for
    /// an incoming request. Waiting does not automatically accept an offer.
    #[serde(default)]
    stage: FileStage,
    after: Option<String>,
    /// Default 60000; 0 waits indefinitely. Maximum nonzero value is 86400000.
    timeout_ms: Option<u64>,
    peer: Option<String>,
    transfer_key: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum WindowAction {
    Show,
    Hide,
    Minimize,
    ToggleMaximize,
    Close,
}

#[derive(Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum WindowLabel {
    #[default]
    Main,
    Receive,
    Preview,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WindowControl {
    #[serde(default)]
    window: WindowLabel,
    action: WindowAction,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DismissPopup {
    /// Clipboard receipt ID, or a completed transfer key. Omit both to dismiss
    /// all informational cards. Pending send/receive confirmations are retained.
    receipt_id: Option<u64>,
    transfer_key: Option<String>,
}

macro_rules! operations {
    ($($variant:ident($args:ty) => ($name:literal, $description:literal, $read:literal, $destructive:literal, $external:literal)),* $(,)?) => {
        #[derive(Deserialize)]
        #[serde(tag = "name", content = "arguments")]
        enum Operation { $(#[serde(rename = $name)] $variant($args)),* }

        pub fn tools() -> Vec<Tool> {
            static TOOLS: std::sync::OnceLock<Vec<Tool>> = std::sync::OnceLock::new();
            TOOLS.get_or_init(|| vec![$({
                let schema = serde_json::to_value(schema_for!($args)).expect("operation schema");
                Tool::new($name, $description, schema.as_object().unwrap().clone())
                    .annotate(ToolAnnotations::new().read_only($read).destructive($destructive).open_world($external))
            }),*]).clone()
        }
    }
}

operations! {
    GetInfo(Empty) => ("get_app_info", "Get AnyDrop version, local API endpoints and event cursor.", true, false, false),
    Snapshot(Empty) => ("get_snapshot", "Read the same service state, peers, settings, transfers and logs shown in the GUI.", true, false, false),
    RefreshPeers(Empty) => ("refresh_peers", "Refresh LAN peer discovery; read get_snapshot again for the results.", false, false, true),
    Start(Empty) => ("start_service", "Start AnyDrop LAN discovery, clipboard sync and file transfer services.", false, false, true),
    Stop(Empty) => ("stop_service", "Stop AnyDrop LAN services. The local API remains available.", false, true, true),
    Settings(SaveSettings) => ("save_settings", "Change GUI settings. Only supplied fields change; network-setting changes restart LAN services.", false, true, false),
    SendClipboard(Empty) => ("send_clipboard_now", "Send the current clipboard to online peers, exactly like the GUI button.", false, false, true),
    SendText(SendText) => ("send_text_to_peer", "Send text to one peer using its hosts from get_snapshot.", false, false, true),
    SendFiles(SendPaths) => ("send_paths", "Offer local files or folders to one peer. The receiver decides whether to accept.", false, false, true),
    Pending(Empty) => ("get_outgoing_requests", "List file-menu and oversized-clipboard requests awaiting send confirmation.", true, false, false),
    Confirm(ConfirmRequest) => ("confirm_outgoing_request", "Confirm one pending send request to a peer name from get_snapshot.", false, false, true),
    DismissRequest(RequestId) => ("dismiss_outgoing_request", "Cancel a pending send confirmation.", false, true, false),
    Accept(AcceptTransfer) => ("accept_transfer", "Accept an incoming file offer into the default or specified directory.", false, true, true),
    Reject(TransferKey) => ("reject_transfer", "Reject an incoming file offer.", false, true, true),
    Cancel(TransferKey) => ("cancel_transfer", "Cancel an in-flight transfer; it cannot be resumed after cancellation.", false, true, true),
    Pause(TransferKey) => ("pause_transfer", "Pause an in-flight transfer, preserving resumable state.", false, false, true),
    Resume(TransferKey) => ("resume_transfer", "Resume a paused or retryable transfer.", false, false, true),
    DismissTransfer(TransferKey) => ("dismiss_transfer", "Remove a completed transfer from history, without deleting received files.", false, true, false),
    ClearTransfers(Empty) => ("clear_transfers", "Clear completed transfer records; preserve active, paused and pending transfers.", false, true, false),
    ClearLogs(Empty) => ("clear_logs", "Clear the GUI log list.", false, true, false),
    Copy(Text) => ("copy_text", "Copy text to the local clipboard without broadcasting it to peers.", false, true, false),
    Remark(PeerRemark) => ("set_peer_remark", "Set a local peer nickname; an empty remark removes it.", false, true, false),
    Reveal(TransferKey) => ("open_transfer_folder", "Reveal a transfer's local file or folder in Finder or Explorer.", false, false, false),
    Directory(Path) => ("open_directory", "Open an absolute directory in the system file manager.", false, false, false),
    Preview(TransferKey) => ("preview_file", "Preview a received file using the GUI's preview action.", false, false, false),
    DesktopPreferences(Empty) => ("get_desktop_preferences", "Read autostart, shortcut, auto-receive, file-menu and updater preferences.", true, false, false),
    Autostart(Enabled) => ("set_autostart", "Enable or disable launching AnyDrop at login.", false, true, false),
    AutoReceive(Enabled) => ("set_auto_receive_files", "Enable or disable automatic acceptance of future incoming file requests.", false, true, false),
    FileMenu(Enabled) => ("set_file_context_menu", "Enable, disable or repair the system file context menu. Setting enabled=true also re-registers it.", false, true, false),
    ClipboardShortcut(Shortcut) => ("set_clipboard_shortcut", "Set the global clipboard-send shortcut; empty string disables it.", false, true, false),
    CheckUpdate(Empty) => ("check_app_update", "Check for a signed application update.", true, false, true),
    DownloadUpdate(Empty) => ("download_app_update", "Download and verify the update previously returned by check_app_update.", false, false, true),
    InstallUpdate(Empty) => ("install_app_update", "Install the verified update and restart AnyDrop. The connection may close during restart.", false, true, false),
    Window(WindowControl) => ("control_window", "Show, hide, minimize, maximize or close a GUI window. Closing the main window hides it.", false, false, false),
    DismissPopup(DismissPopup) => ("dismiss_popup", "Dismiss received clipboard or completed-file popup cards, keeping pending confirmations.", false, false, false),
    Quit(Empty) => ("quit_app", "Exit AnyDrop, stopping both LAN services and this local API.", false, true, false),
    Cursor(Empty) => ("get_event_cursor", "Get a cursor before triggering a transfer, then pass it as after to a wait operation.", true, false, false),
    WaitFile(WaitFile) => ("wait_for_file", "Wait for the next incoming file transfer to finish and return its local paths. stage=offered waits for a request instead. Does not change auto-receive settings.", true, false, false),
    WaitText(WaitOptions) => ("wait_for_clipboard_text", "Wait for the next clipboard text received from another device, returning the complete text and peer. Local copies do not satisfy this wait.", true, false, false),
}

fn value(value: impl Serialize) -> Result<Value, ApiError> {
    serde_json::to_value(value).map_err(|e| ApiError::internal(e.to_string()))
}

fn absolute(path: &str) -> Result<(), ApiError> {
    if !std::path::Path::new(path).is_absolute() {
        return Err(ApiError::invalid("An absolute local path is required"));
    }
    Ok(())
}

fn transfer_action_allowed(
    backend: &Backend,
    key: &str,
    statuses: &[u8],
    incoming: bool,
) -> Result<(), ApiError> {
    let transfers = backend.transfers.lock().unwrap();
    let transfer = transfers
        .get(key)
        .ok_or_else(|| ApiError::invalid("Transfer not found"))?;
    if !statuses.contains(&transfer.status) || (incoming && transfer.direction != "incoming") {
        return Err(ApiError::invalid(
            "This action is not available for the transfer's current state",
        ));
    }
    Ok(())
}

pub async fn call(
    app: AppHandle,
    name: String,
    arguments: Value,
    cancellation: CancellationToken,
) -> Result<Value, ApiError> {
    if !tools().iter().any(|tool| tool.name == name) {
        return Err(ApiError::not_found(&name));
    }
    let operation: Operation =
        serde_json::from_value(json!({"name": name, "arguments": arguments}))
            .map_err(|e| ApiError::invalid(e.to_string()))?;
    match operation {
        Operation::WaitFile(args) => {
            let kind = match args.stage {
                FileStage::Offered => Kind::FileOffered,
                FileStage::Completed => Kind::FileReceived,
            };
            let events = app.state::<Backend>().events.clone();
            value(
                events
                    .wait(
                        kind,
                        WaitOptions {
                            after: args.after,
                            timeout_ms: args.timeout_ms,
                            peer: args.peer,
                            transfer_key: args.transfer_key,
                        },
                        cancellation,
                    )
                    .await?,
            )
        }
        Operation::WaitText(args) => {
            let events = app.state::<Backend>().events.clone();
            value(events.wait(Kind::ClipboardText, args, cancellation).await?)
        }
        Operation::SendClipboard(_) => value(crate::send_clipboard_now(app).await?),
        Operation::Confirm(args) => {
            value(outgoing::confirm_outgoing_request(app, args.id, args.peer_name).await?)
        }
        Operation::Autostart(args) => value(desktop::set_autostart(app, args.enabled).await?),
        Operation::AutoReceive(args) => {
            value(desktop::set_auto_receive_files(app, args.enabled).await?)
        }
        Operation::FileMenu(args) => {
            value(desktop::set_file_context_menu(app, args.enabled).await?)
        }
        Operation::ClipboardShortcut(args) => {
            value(desktop::set_clipboard_shortcut(app, args.shortcut).await?)
        }
        Operation::CheckUpdate(_) => {
            value(updates::check_app_update(app.clone(), app.state()).await?)
        }
        Operation::DownloadUpdate(_) => {
            value(updates::download_app_update(app.clone(), app.state()).await?)
        }
        Operation::InstallUpdate(_) => value(updates::install_app_update(app).await?),
        Operation::Preview(args) => {
            value(crate::preview_file(app.clone(), app.state(), args.transfer_key).await?)
        }
        Operation::Window(args) => window_action(app, args).await,
        other => tauri::async_runtime::spawn_blocking(move || call_sync(app, other))
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?,
    }
}

fn call_sync(app: AppHandle, operation: Operation) -> Result<Value, ApiError> {
    let backend = app.state::<Backend>();
    match operation {
        Operation::GetInfo(_) => Ok(
            json!({"name":"AnyDrop", "version":app.package_info().version.to_string(), "platform":std::env::consts::OS,
            "mcp":format!("http://localhost:{}/mcp", super::port()), "api":format!("http://localhost:{}/api", super::port()), "eventCursor":backend.events.cursor()}),
        ),
        Operation::Snapshot(_) => value(backend.snapshot()),
        Operation::RefreshPeers(_) => value(crate::refresh_peers(backend)?),
        Operation::Start(_) => value(crate::start_service(app.clone(), backend)?),
        Operation::Stop(_) => value(crate::stop_service(app.clone(), backend)),
        Operation::Settings(args) => {
            let _change = backend.settings_change.lock().unwrap();
            let mut settings = backend.settings.lock().unwrap().clone();
            args.settings.apply(&mut settings)?;
            value(crate::save_settings_inner(&app, &backend, settings)?)
        }
        Operation::SendText(args) => value(crate::send_text_to_peer(
            app.clone(),
            backend,
            args.hosts,
            args.text,
        )?),
        Operation::SendFiles(args) => {
            for path in &args.paths {
                absolute(path)?;
            }
            let (snapshot, key) = crate::send_paths_inner(&app, &backend, args.hosts, args.paths)?;
            Ok(json!({"transferKey": key, "snapshot": snapshot}))
        }
        Operation::Pending(_) => value(outgoing::get_outgoing_requests(backend)),
        Operation::DismissRequest(args) => {
            outgoing::dismiss_outgoing_request(app.clone(), backend, args.id);
            Ok(Value::Null)
        }
        Operation::Accept(args) => {
            if let Some(path) = &args.save_dir {
                absolute(path)?;
            }
            value(crate::accept_transfer(
                app.clone(),
                backend,
                args.transfer_key,
                args.save_dir,
            )?)
        }
        Operation::Reject(args) => {
            transfer_action_allowed(&backend, &args.transfer_key, &[1], true)?;
            value(crate::reject_transfer(
                app.clone(),
                backend,
                args.transfer_key,
            )?)
        }
        Operation::Cancel(args) => {
            transfer_action_allowed(&backend, &args.transfer_key, &[4, 9, 10], false)?;
            value(crate::cancel_transfer(
                app.clone(),
                backend,
                args.transfer_key,
            )?)
        }
        Operation::Pause(args) => {
            transfer_action_allowed(&backend, &args.transfer_key, &[4], false)?;
            value(crate::pause_transfer(
                app.clone(),
                backend,
                args.transfer_key,
            )?)
        }
        Operation::Resume(args) => {
            transfer_action_allowed(&backend, &args.transfer_key, &[9], false)?;
            value(crate::resume_transfer(
                app.clone(),
                backend,
                args.transfer_key,
            )?)
        }
        Operation::DismissTransfer(args) => {
            if backend
                .transfers
                .lock()
                .unwrap()
                .get(&args.transfer_key)
                .is_some_and(|t| !crate::is_terminal_status(t.status))
            {
                return Err(ApiError::invalid(
                    "Complete or cancel this transfer before removing its record",
                ));
            }
            value(crate::dismiss_transfer(
                app.clone(),
                backend,
                args.transfer_key,
            ))
        }
        Operation::ClearTransfers(_) => value(crate::clear_transfers(app.clone(), backend)),
        Operation::ClearLogs(_) => value(crate::clear_logs(app.clone(), backend)),
        Operation::Copy(args) => value(crate::copy_text(backend, args.text)?),
        Operation::Remark(args) => value(crate::set_peer_remark(
            app.clone(),
            backend,
            args.peer_key,
            args.remark,
        )?),
        Operation::Reveal(args) => value(crate::open_transfer_folder(
            app.clone(),
            backend,
            args.transfer_key,
        )?),
        Operation::Directory(args) => {
            absolute(&args.path)?;
            value(crate::open_directory(args.path)?)
        }
        Operation::DesktopPreferences(_) => {
            value(desktop::get_desktop_preferences(app.clone(), app.state())?)
        }
        Operation::Cursor(_) => Ok(json!({"cursor":backend.events.cursor()})),
        Operation::DismissPopup(args) => {
            if args.receipt_id.is_some() && args.transfer_key.is_some() {
                return Err(ApiError::invalid(
                    "Specify receiptId or transferKey, not both",
                ));
            }
            let all_receipts = args.receipt_id.is_none() && args.transfer_key.is_none();
            if let Some(id) = args.receipt_id {
                receipts::acknowledge_clipboard_receipts(app.state(), vec![id]);
            }
            if all_receipts {
                receipts::dismiss_all_clipboard_receipts(&backend);
            }
            let transfers = backend.transfer_list();
            if args.transfer_key.as_ref().is_some_and(|key| {
                transfers
                    .iter()
                    .any(|t| &t.key == key && !crate::is_terminal_status(t.status))
            }) {
                return Err(ApiError::invalid(
                    "Pending and active file cards cannot be dismissed",
                ));
            }
            let keys: Vec<_> = transfers
                .iter()
                .filter(|t| {
                    crate::is_terminal_status(t.status)
                        && args.transfer_key.as_ref().is_none_or(|key| &t.key == key)
                        && args.receipt_id.is_none()
                })
                .map(|t| &t.key)
                .collect();
            app.emit_to(
                crate::RECEIVE_WINDOW_LABEL,
                "dismiss-popup",
                json!({"receiptId":args.receipt_id,"transferKeys":keys,"allReceipts":all_receipts}),
            )
            .map_err(|e| ApiError::internal(e.to_string()))?;
            Ok(Value::Null)
        }
        Operation::Quit(_) => {
            // Give the successful local response time to leave before exit.
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                app.exit(0);
            });
            Ok(json!({"exiting":true}))
        }
        _ => Err(ApiError::internal(
            "Async operation dispatched synchronously",
        )),
    }
}

async fn window_action(app: AppHandle, args: WindowControl) -> Result<Value, ApiError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.clone()
        .run_on_main_thread(move || {
            let result = (|| -> Result<(), String> {
                let label = match args.window {
                    WindowLabel::Main => "main",
                    WindowLabel::Receive => crate::RECEIVE_WINDOW_LABEL,
                    WindowLabel::Preview => crate::PREVIEW_WINDOW_LABEL,
                };
                let window = app.get_webview_window(label).ok_or("Window is not open")?;
                match args.action {
                    WindowAction::Show => {
                        if label == crate::RECEIVE_WINDOW_LABEL {
                            return crate::show_receive_window(&window);
                        } else {
                            window
                                .show()
                                .and_then(|_| window.unminimize())
                                .and_then(|_| window.set_focus())
                        }
                    }
                    WindowAction::Hide => window.hide(),
                    WindowAction::Minimize => window.minimize(),
                    WindowAction::ToggleMaximize => {
                        if window.is_maximized().map_err(|e| e.to_string())? {
                            window.unmaximize()
                        } else {
                            window.maximize()
                        }
                    }
                    WindowAction::Close => window.close(),
                }
                .map_err(|e| e.to_string())
            })();
            let _ = sender.send(result);
        })
        .map_err(|e| ApiError::internal(e.to_string()))?;
    value(
        receiver
            .await
            .map_err(|e| ApiError::internal(e.to_string()))??,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_settings_preserve_unmentioned_values_and_can_restore_system_theme() {
        let mut settings = AppSettings::default();
        settings.dark_mode = Some(true);
        settings.display_name = "my-device".into();
        let patch: SettingsChange =
            serde_json::from_value(json!({"darkMode":null,"clipboardPopupEnabled":true})).unwrap();
        patch.apply(&mut settings).unwrap();
        assert_eq!(settings.dark_mode, None);
        assert_eq!(settings.display_name, "my-device");
        assert!(settings.clipboard_popup_enabled);
        assert!(serde_json::from_value::<SettingsChange>(json!({"unknown":true})).is_err());
    }

    #[test]
    fn catalogue_has_unique_names_strict_arguments_and_completed_file_wait_default() {
        let tools = tools();
        let names: std::collections::HashSet<_> = tools.iter().map(|tool| &tool.name).collect();
        assert_eq!(names.len(), tools.len());
        let wait: WaitFile = serde_json::from_value(json!({})).unwrap();
        assert!(matches!(wait.stage, FileStage::Completed));
        assert!(serde_json::from_value::<Operation>(
            json!({"name":"get_snapshot","arguments":{"unexpected":1}})
        )
        .is_err());
        for tool in tools {
            assert_eq!(tool.input_schema["additionalProperties"], false);
        }
    }
}
