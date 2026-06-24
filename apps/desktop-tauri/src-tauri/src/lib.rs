use anydrop::lib_util::{shared_anydrop_broadcast_text, shared_anydrop_init, CONNECTION_TIMEOUT_MILLIS};
use anydrop::network::peer::Peer;
use anydrop::packet::data::magic_numbers::MagicNumbers;
use anydrop::packet::data::text_packet::TextPacket;
use anydrop::packet::protocol::serialize::Serialize;
use anydrop::service::anydrop_service::{AnyDropService, AnyDropServiceConfig};
use anydrop::service::context::data_service_context::DataServiceContext;
use anydrop::service::data_service::DataService;
use anydrop::service::discovery_service::DiscoveryService;
use anydrop::transfer::{
    self, Decision, Direction as TransferDirection, ProgressUpdate as TransferProgress,
    ServerHandle as TransferServerHandle, TransferOffer, TransferStatus,
};
use anydrop::util::os::OSUtil;
use clipboard_master::{CallbackResult, ClipboardHandler, Master};
use serde::{Deserialize, Serialize as SerdeSerialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fs;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, State, UserAttentionType, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

const DEFAULT_DISCOVERY_PORT: u16 = 9818;
const DEFAULT_DATA_PORT: u16 = 9819;

/// Bottom-right native popup that surfaces incoming offers / receive progress
/// (and optionally clipboard receipts). Created lazily, reused across events.
const RECEIVE_WINDOW_LABEL: &str = "receive";
const RECEIVE_WINDOW_WIDTH: f64 = 380.0;
const RECEIVE_WINDOW_HEIGHT: f64 = 440.0;
/// Quick-Look style preview window for received media.
const PREVIEW_WINDOW_LABEL: &str = "preview";
/// Gap from the screen edges when docking the receive popup bottom-right.
const POPUP_MARGIN: f64 = 16.0;

fn config_base() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("AnyDrop")
}

fn settings_path() -> Result<PathBuf, String> {
    let base = config_base();
    fs::create_dir_all(&base).map_err(|err| err.to_string())?;
    Ok(base.join("settings.json"))
}

fn peer_remarks_path() -> Result<PathBuf, String> {
    let base = config_base();
    fs::create_dir_all(&base).map_err(|err| err.to_string())?;
    Ok(base.join("peer_remarks.json"))
}

/// Default location for received files: `~/Downloads/AnyDrop`. Used both as the
/// `default_save_dir` seed and the ultimate fallback when settings are missing.
fn default_save_root() -> PathBuf {
    dirs::download_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("AnyDrop")
}

fn load_peer_remarks() -> HashMap<String, String> {
    let Ok(path) = peer_remarks_path() else {
        return HashMap::new();
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    serde_json::from_str::<HashMap<String, String>>(&raw).unwrap_or_default()
}

fn save_peer_remarks_file(remarks: &HashMap<String, String>) -> Result<(), String> {
    let path = peer_remarks_path()?;
    let raw = serde_json::to_string_pretty(remarks).map_err(|err| err.to_string())?;
    fs::write(path, raw).map_err(|err| err.to_string())
}

fn transfers_path() -> Result<PathBuf, String> {
    let base = config_base();
    fs::create_dir_all(&base).map_err(|err| err.to_string())?;
    Ok(base.join("transfers.json"))
}

/// A transfer status that won't change on its own (no live session driving it).
fn is_terminal_status(status: u8) -> bool {
    matches!(status, 2 | 5 | 6 | 7 | 8)
}

/// Persist the transfer records (history) so they survive restarts. Best-effort.
fn save_transfers(transfers: &HashMap<String, Transfer>) {
    let Ok(path) = transfers_path() else {
        return;
    };
    let list: Vec<&Transfer> = transfers.values().collect();
    if let Ok(raw) = serde_json::to_string(&list) {
        let _ = fs::write(path, raw);
    }
}

/// Load persisted transfer records. In-flight/pending records can't survive a
/// restart (the QUIC session is gone), so pending offers are dropped and any
/// non-terminal record is marked interrupted.
fn load_transfers() -> HashMap<String, Transfer> {
    let mut map = HashMap::new();
    let Ok(path) = transfers_path() else {
        return map;
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return map;
    };
    let Ok(list) = serde_json::from_str::<Vec<Transfer>>(&raw) else {
        return map;
    };
    for mut t in list {
        if t.status == 1 {
            continue; // a pending offer is meaningless after restart
        }
        if !is_terminal_status(t.status) {
            t.status = 6; // in-progress / paused → interrupted by restart
            t.error = Some("应用重启，传输中断".to_string());
            t.speed_bps = 0.0;
        }
        map.insert(t.key.clone(), t);
    }
    map
}

fn normalize_settings(mut settings: AppSettings) -> AppSettings {
    if settings.discovery_port == 0 {
        settings.discovery_port = DEFAULT_DISCOVERY_PORT;
    }
    if settings.data_port == 0 {
        settings.data_port = DEFAULT_DATA_PORT;
    }
    if settings.display_name.trim().is_empty() {
        settings.display_name = OSUtil::hostname();
    }
    if settings.default_save_dir.trim().is_empty() {
        settings.default_save_dir = default_save_root().to_string_lossy().to_string();
    }
    if settings.device_id.trim().is_empty() {
        // 128-bit random hex; stable for the life of the install once persisted.
        settings.device_id = format!("{:032x}", rand::random::<u128>());
    }
    settings
}

fn load_settings() -> AppSettings {
    let settings = (|| {
        let path = settings_path().ok()?;
        let raw = fs::read_to_string(path).ok()?;
        serde_json::from_str::<AppSettings>(&raw).ok()
    })()
    .map(normalize_settings)
    .unwrap_or_else(|| normalize_settings(AppSettings::default()));
    // Persist back so freshly-generated fields (notably device_id) survive the
    // next launch. Best-effort: a write failure just means we regenerate later.
    let _ = save_settings_file(&settings);
    settings
}

fn save_settings_file(settings: &AppSettings) -> Result<(), String> {
    let path = settings_path()?;
    let raw = serde_json::to_string_pretty(settings).map_err(|err| err.to_string())?;
    fs::write(path, raw).map_err(|err| err.to_string())
}

#[derive(Clone, Deserialize, SerdeSerialize)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    send_clipboard_enabled: bool,
    receive_clipboard_enabled: bool,
    send_only_on_double_copy: bool,
    group_identity: u32,
    discovery_port: u16,
    data_port: u16,
    /// Display name advertised to peers. Defaults to the system hostname.
    #[serde(default)]
    display_name: String,
    /// Whether to broadcast / receive clipboard *images* alongside text.
    /// Default off — images are larger and more privacy-sensitive than text.
    #[serde(default)]
    sync_image_enabled: bool,
    /// UI theme preference. `None` = follow the OS theme (first-run default);
    /// `Some(true/false)` = explicit dark / light chosen by the user. Persisted
    /// here (not in browser localStorage) so the choice is native-stored.
    #[serde(default)]
    dark_mode: Option<bool>,
    /// Default directory received files land in. normalize_settings seeds it to
    /// `~/Downloads/AnyDrop` when empty.
    #[serde(default)]
    default_save_dir: String,
    /// Show a bottom-right popup (like the file-receive one) when clipboard
    /// content arrives from a peer. Default off — keeps the current silent
    /// behaviour unless the user opts in.
    #[serde(default)]
    clipboard_popup_enabled: bool,
    /// Stable per-install device id. Advertised in discovery so a device's
    /// multiple addresses / hostnames collapse into one peer, and used as the
    /// local-remark key (survives IP and hostname changes). Generated once by
    /// normalize_settings when empty. Not surfaced in the UI.
    #[serde(default)]
    device_id: String,
    /// Suppress popups / notifications while a fullscreen game (exclusive or
    /// borderless) is in the foreground. Default on.
    #[serde(default = "default_true")]
    suppress_popup_in_game: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            send_clipboard_enabled: true,
            receive_clipboard_enabled: true,
            send_only_on_double_copy: false,
            group_identity: 0,
            discovery_port: DEFAULT_DISCOVERY_PORT,
            data_port: DEFAULT_DATA_PORT,
            display_name: String::new(), // normalize_settings fills from OSUtil
            sync_image_enabled: false,
            dark_mode: None,
            default_save_dir: String::new(), // normalize_settings fills default
            clipboard_popup_enabled: false,
            device_id: String::new(), // normalize_settings generates one
            suppress_popup_in_game: true,
        }
    }
}

#[derive(Clone, SerdeSerialize)]
#[serde(rename_all = "camelCase")]
struct PeerGroup {
    label: String,
    name: String,
    hosts: Vec<String>,
    /// Local-only nickname for this peer, keyed by the group key / device id
    /// (`name`). Overlaid at snapshot time from `Backend::peer_remarks`; never
    /// advertised, so the remote device cannot see how it has been labelled.
    #[serde(skip_serializing_if = "Option::is_none")]
    remark: Option<String>,
}

#[derive(Clone, Deserialize, SerdeSerialize)]
#[serde(rename_all = "camelCase")]
struct Transfer {
    key: String,
    file_id: u8,
    file_name: String,
    remote_path: String,
    local_path: String,
    peer: String,
    host: String,
    direction: String,
    progress: u64,
    total: u64,
    status: u8,
    /// Latest error string reported for this transfer, if any. Set when a
    /// receive task fails (invalid filename, disk full, permission denied,
    /// …) or when the peer aborts. Cleared only on a fresh Transfer entry —
    /// once we've shown an error, we keep showing it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// Smoothed instantaneous transfer rate in bytes per second. EWMA-blended
    /// across progress events so the UI shows a stable number instead of
    /// jittery per-chunk samples. Reset to 0 at terminal states.
    #[serde(default)]
    speed_bps: f64,
    /// Bookkeeping for the EWMA computation — not serialized.
    #[serde(skip)]
    speed_last_at: Option<std::time::Instant>,
    #[serde(skip)]
    speed_last_bytes: u64,
    /// Top-level entry name of the transfer (the first path segment of the
    /// first item). For a single-file send this is the file name; for a folder
    /// send it's the folder name. Used to compute `reveal_path` once a save
    /// root is known. Persisted so reveal/preview keep working after a restart.
    #[serde(default)]
    top_level: String,
    /// Absolute path to reveal/select in the file manager and to preview:
    /// `save_root/top_level` on the receive side, the source path on the send
    /// side. Empty until known. Persisted across restarts.
    #[serde(default)]
    reveal_path: String,
}

#[derive(Clone, SerdeSerialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    running: bool,
    settings: AppSettings,
    peers: Vec<PeerGroup>,
    transfers: Vec<Transfer>,
    last_clipboard_text: String,
    last_received_text: String,
    status_text: String,
    logs: Vec<String>,
}

struct ClipboardState {
    last_text: String,
    last_received_text: String,
    suppressed_text: Option<String>,
    /// 8-byte FNV-1a digest of the most recently set raw RGBA image we
    /// pushed to the local clipboard. Used to detect the OS clipboard
    /// change event that comes back from our own set_image — without this
    /// we'd ping-pong the same image around the network.
    suppressed_image_digest: Option<u64>,
    /// Digest of the last broadcast image; used to skip "same image
    /// re-copied" events (mirror of `last_text` for images).
    last_image_digest: Option<u64>,
    /// When `last_received_text` was last accepted. Used to drop rapid network
    /// duplicates: a sender broadcasts the same text to every address of each
    /// peer, so a device known under two addresses receives it twice.
    last_received_at: Option<std::time::Instant>,
}

impl Default for ClipboardState {
    fn default() -> Self {
        Self {
            last_text: String::new(),
            last_received_text: String::new(),
            suppressed_text: None,
            suppressed_image_digest: None,
            last_image_digest: None,
            last_received_at: None,
        }
    }
}

/// Cheap, non-cryptographic 64-bit FNV-1a digest. We only need
/// "same/different" semantics for clipboard loopback suppression — not
/// security — so we trade hash quality for zero deps.
fn sha256_short(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

struct ServiceRuntime {
    service: Arc<AnyDropService>,
    config: AnyDropServiceConfig,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl ServiceRuntime {
    fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

#[derive(Default)]
struct Backend {
    runtime: Mutex<Option<ServiceRuntime>>,
    settings: Mutex<AppSettings>,
    peers: Arc<Mutex<Vec<PeerGroup>>>,
    transfers: Arc<Mutex<HashMap<String, Transfer>>>,
    clipboard: Arc<Mutex<ClipboardState>>,
    status_text: Mutex<String>,
    log_entries: Arc<Mutex<VecDeque<String>>>,
    /// Local-only peer nicknames keyed by device id (the peer group key).
    /// Loaded from `peer_remarks.json`, overlaid onto peer groups at snapshot.
    peer_remarks: Arc<Mutex<HashMap<String, String>>>,
    /// QUIC transfer server. Started alongside the legacy text data service
    /// when the runtime spins up.
    transfer_handle: Mutex<Option<Arc<TransferServerHandle>>>,
    /// Payload (path / kind / name) for the most recent preview request. The
    /// preview window reads it on mount via `get_preview_payload`, avoiding a
    /// build-then-emit race and URL-encoding of arbitrary file paths.
    preview_payload: Arc<Mutex<Option<serde_json::Value>>>,
}

/// Build the `Transfer.key` for a given transfer_id. Currently a plain decimal
/// string; the "v2:" prefix used during the migration is gone now that v1
/// transfers no longer exist.
fn make_transfer_key(transfer_id: u64) -> String {
    transfer_id.to_string()
}

fn transfer_status_to_u8(status: TransferStatus) -> u8 {
    match status {
        TransferStatus::PendingDecision => 1,
        TransferStatus::InProgress | TransferStatus::ItemDone => 4,
        TransferStatus::AllDone => 7,
        TransferStatus::Error => 6,
        TransferStatus::Rejected => 2,
        TransferStatus::Cancelled => 5,
        TransferStatus::Paused => 9,
        // Sender-side: waiting for the peer to accept/reject.
        TransferStatus::AwaitingAccept => 10,
    }
}

fn offer_label(offer: &TransferOffer) -> String {
    let files: Vec<&transfer::Item> = offer.items.iter().filter(|i| !i.is_dir).collect();
    if files.len() == 1 {
        return file_name(&files[0].rel_path);
    }
    if files.is_empty() {
        return offer
            .items
            .first()
            .map(|i| file_name(&i.rel_path))
            .unwrap_or_else(|| "传输".to_string());
    }
    let first = file_name(&files[0].rel_path);
    format!("{} 等 {} 个项目", first, files.len())
}

/// Convert a transfer progress event into a Transfer row the UI can render.
fn apply_progress(
    transfers: &Arc<Mutex<HashMap<String, Transfer>>>,
    p: &TransferProgress,
    initial_label: Option<&str>,
) -> Transfer {
    let key = make_transfer_key(p.transfer_id);
    let mut map = transfers.lock().unwrap();
    let entry = map.entry(key.clone()).or_insert_with(|| Transfer {
        key: key.clone(),
        file_id: 0,
        file_name: initial_label
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                if p.rel_path.is_empty() {
                    format!("传输 #{}", p.transfer_id)
                } else {
                    file_name(&p.rel_path)
                }
            }),
        remote_path: format!("transfer_id:{}", p.transfer_id),
        local_path: String::new(),
        peer: format!("{}@{}", p.display_name, p.remote_addr),
        host: p.remote_addr.ip().to_string(),
        direction: if matches!(p.direction, TransferDirection::Send) {
            "outgoing".to_string()
        } else {
            "incoming".to_string()
        },
        progress: 0,
        total: p.total_size,
        status: transfer_status_to_u8(p.status),
        error: None,
        speed_bps: 0.0,
        speed_last_at: None,
        speed_last_bytes: 0,
        top_level: if p.rel_path.is_empty() {
            String::new()
        } else {
            top_level_name(&p.rel_path)
        },
        reveal_path: String::new(),
    });
    // Compute smoothed instantaneous speed before we overwrite progress. EWMA
    // with α=0.3 — enough new-sample weight to react to real changes, enough
    // history to dampen per-chunk jitter at our 200 ms throttle.
    let now = std::time::Instant::now();
    if let Some(last_at) = entry.speed_last_at {
        let dt = now.duration_since(last_at).as_secs_f64();
        if dt > 0.0 {
            let dbytes = p.total_done.saturating_sub(entry.speed_last_bytes);
            let sample = dbytes as f64 / dt;
            entry.speed_bps = if entry.speed_bps == 0.0 {
                sample
            } else {
                entry.speed_bps * 0.7 + sample * 0.3
            };
        }
    }
    entry.speed_last_at = Some(now);
    entry.speed_last_bytes = p.total_done;

    // First non-empty rel_path on a row we pre-inserted (sender path) becomes
    // the display label — typically the client's synthetic "summary" event
    // carrying something like "MyFolder (3 个文件)". Per-item events that
    // follow have rel_path like "MyFolder/foo.txt" which we don't want to
    // overwrite the summary with, hence the empty-check.
    if entry.file_name.is_empty() && !p.rel_path.is_empty() {
        entry.file_name = file_name(&p.rel_path);
    }

    entry.progress = p.total_done;
    if p.total_size > 0 {
        entry.total = p.total_size;
    }
    entry.status = transfer_status_to_u8(p.status);
    // Zero the speed at terminal states so the UI doesn't keep showing the
    // last instant rate after completion.
    if matches!(p.status, TransferStatus::AllDone | TransferStatus::Error | TransferStatus::Rejected)
    {
        entry.speed_bps = 0.0;
    }
    // Sticky: once we've recorded an error, surface it for the rest of the
    // transfer's lifetime so the user can see what went wrong. New errors
    // overwrite old ones (most recent diagnosis wins).
    if let Some(err) = p.error.as_ref() {
        entry.error = Some(err.clone());
    }
    entry.clone()
}

impl Backend {
    fn new(settings: AppSettings) -> Self {
        Self {
            settings: Mutex::new(settings),
            peer_remarks: Arc::new(Mutex::new(load_peer_remarks())),
            transfers: Arc::new(Mutex::new(load_transfers())),
            ..Self::default()
        }
    }

    fn is_running(&self) -> bool {
        self.runtime
            .try_lock()
            .map(|runtime| runtime.is_some())
            .unwrap_or(false)
    }

    fn snapshot(&self) -> Snapshot {
        let settings = self
            .settings
            .try_lock()
            .map(|settings| settings.clone())
            .unwrap_or_default();
        let mut peers = self
            .peers
            .try_lock()
            .map(|peers| peers.clone())
            .unwrap_or_default();
        // Overlay local nicknames fresh on every snapshot so editing a remark
        // takes effect immediately without rebuilding the cached peer list.
        if let Ok(remarks) = self.peer_remarks.try_lock() {
            for group in peers.iter_mut() {
                group.remark = remarks
                    .get(&group.name)
                    .filter(|r| !r.trim().is_empty())
                    .cloned();
            }
        }
        let (last_clipboard_text, last_received_text) = self
            .clipboard
            .try_lock()
            .map(|clipboard| {
                (
                    clipboard.last_text.clone(),
                    clipboard.last_received_text.clone(),
                )
            })
            .unwrap_or_default();
        let status_text = self
            .status_text
            .try_lock()
            .map(|status| status.clone())
            .unwrap_or_else(|_| "Busy".to_string());
        let logs = self
            .log_entries
            .try_lock()
            .map(|l| l.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        Snapshot {
            running: self.is_running(),
            settings,
            peers,
            transfers: self.transfer_list(),
            last_clipboard_text,
            last_received_text,
            status_text,
            logs,
        }
    }

    fn set_status(&self, text: impl Into<String>) {
        *self.status_text.lock().unwrap() = text.into();
    }

    fn log(&self, msg: impl Into<String>) {
        add_log(&self.log_entries, msg);
    }

    fn config(&self) -> Option<AnyDropServiceConfig> {
        self.runtime
            .lock()
            .unwrap()
            .as_ref()
            .map(|runtime| runtime.config.clone())
    }

    fn service(&self) -> Option<Arc<AnyDropService>> {
        self.runtime
            .lock()
            .unwrap()
            .as_ref()
            .map(|runtime| runtime.service.clone())
    }

    fn transfer_list(&self) -> Vec<Transfer> {
        let Ok(transfers) = self.transfers.try_lock() else {
            return Vec::new();
        };
        let mut transfers = transfers.values().cloned().collect::<Vec<_>>();
        transfers.sort_by(|a, b| a.key.cmp(&b.key));
        transfers
    }

    /// Persist the current transfer records to disk (history survives restart).
    fn persist_transfers(&self) {
        if let Ok(map) = self.transfers.lock() {
            save_transfers(&map);
        }
    }
}

fn emit_snapshot(app: &AppHandle) {
    if let Some(backend) = app.try_state::<Backend>() {
        let _ = app.emit("snapshot", backend.snapshot());
    }
}

fn file_name(path: &str) -> String {
    // Cross-platform basename: Path::file_name() on Unix only treats '/' as a
    // separator, so Windows paths like "C:\Users\foo.txt" survive intact on
    // macOS. Manually split on both separators to handle peer paths from any
    // platform.
    path.rsplit(|c| c == '/' || c == '\\')
        .find(|s| !s.is_empty())
        .unwrap_or("received-file")
        .to_string()
}

fn add_log(entries: &Arc<Mutex<VecDeque<String>>>, msg: impl Into<String>) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| {
            let s = d.as_secs();
            let h = (s % 86400) / 3600;
            let m = (s % 3600) / 60;
            let sc = s % 60;
            let ms = d.subsec_millis();
            format!("{h:02}:{m:02}:{sc:02}.{ms:03}")
        })
        .unwrap_or_else(|_| "??:??:??.???".to_string());
    if let Ok(mut log) = entries.lock() {
        log.push_back(format!("[{ts}] {}", msg.into()));
        while log.len() > 200 {
            log.pop_front();
        }
    }
}

fn peer_text(peer: Option<&Peer>) -> String {
    peer.map(ToString::to_string)
        .unwrap_or_else(|| Peer::default().to_string())
}

fn port_available(discovery_port: u16, data_port: u16) -> Result<(), String> {
    UdpSocket::bind((Ipv4Addr::UNSPECIFIED, discovery_port))
        .map_err(|err| format!("Discovery port {discovery_port} is unavailable: {err}"))?;
    TcpListener::bind((Ipv4Addr::UNSPECIFIED, data_port))
        .map_err(|err| format!("Data port {data_port} is unavailable: {err}"))?;
    Ok(())
}

/// Parse a host string list into IPv4 candidates, silently dropping anything
/// non-IPv4. Used as a feeder for `anydrop::util::network::pick_best_peer`.
fn parse_ipv4_list(hosts: &[String]) -> Vec<std::net::Ipv4Addr> {
    hosts
        .iter()
        .filter_map(|h| h.parse::<std::net::Ipv4Addr>().ok())
        .collect()
}

/// Pick the most reachable peer host string from a candidate list.
///
/// Same-subnet match first, TCP probe fallback to `data_port` second,
/// best-ranked candidate as a last-resort hand-off third.
fn best_reachable_host(hosts: &[String], data_port: u16) -> Option<String> {
    let candidates = parse_ipv4_list(hosts);
    anydrop::util::network::pick_best_peer(
        &candidates,
        data_port,
        Duration::from_millis(500),
    )
    .map(|sa| sa.ip().to_string())
}

fn group_peers(peers: impl Iterator<Item = Peer>) -> Vec<PeerGroup> {
    // Group by stable device id: a device's multiple addresses and/or
    // advertised hostnames collapse into one peer. Peers that predate the
    // device_id field (empty) fall back to grouping by IP so they still show
    // up. The group key (`name`) doubles as the remark key — keyed by device
    // id, so a remark survives the device's IP and hostname changing.
    #[derive(Default)]
    struct Agg {
        addrs: BTreeSet<String>,
        hostnames: BTreeSet<String>,
    }
    let mut groups: BTreeMap<String, Agg> = BTreeMap::new();
    for peer in peers {
        let dev = peer.device_id().clone();
        let key = if dev.is_empty() {
            format!("ip:{}", peer.host())
        } else {
            format!("dev:{dev}")
        };
        let agg = groups.entry(key).or_default();
        agg.addrs.insert(peer.host().clone());
        let name = peer.host_name().clone();
        if !name.is_empty() && name != "<empty>" {
            agg.hostnames.insert(name);
        }
    }

    groups
        .into_iter()
        .map(|(key, agg)| {
            let hosts: Vec<String> = agg.addrs.into_iter().collect();
            let hostnames: Vec<String> = agg.hostnames.into_iter().collect();
            // Representative name: a hostname if known, else the first address.
            let base = hostnames
                .first()
                .cloned()
                .or_else(|| hosts.first().cloned())
                .unwrap_or_default();
            // Append count hints, each only when > 1: e.g.
            // "foo", "foo (2个地址)", "foo (2个主机)", "foo (2个地址, 2个主机)".
            let mut parts: Vec<String> = Vec::new();
            if hosts.len() > 1 {
                parts.push(format!("{}个地址", hosts.len()));
            }
            if hostnames.len() > 1 {
                parts.push(format!("{}个主机", hostnames.len()));
            }
            let label = if parts.is_empty() {
                base
            } else {
                format!("{} ({})", base, parts.join(", "))
            };
            PeerGroup {
                label,
                name: key, // group key = device id (remark key); never shown
                hosts,
                remark: None, // filled at snapshot time from peer_remarks
            }
        })
        .collect()
}

/// Top-level entry name of a transfer = the first path segment of a relative
/// path. For `"Folder/sub/file.txt"` → `"Folder"`; for `"file.txt"` → the file
/// itself. Used to compute the reveal/preview target under the save root.
fn top_level_name(rel: &str) -> String {
    rel.split(|c| c == '/' || c == '\\')
        .find(|s| !s.is_empty())
        .unwrap_or(rel)
        .to_string()
}

/// Window inside which two consecutive clipboard updates count as a "double
/// copy" gesture (≈ user hammering Ctrl+C twice).  Long enough to forgive
/// shaky fingers, short enough not to chain unrelated copies together.
const DOUBLE_COPY_WINDOW_MS: u128 = 600;

/// Adapter that turns clipboard-master's per-event callback into our existing
/// send-text plumbing. Holds enough state to:
///   * detect double-copy gestures (last_event_at)
///   * suppress the echo when we just pasted text received from a peer
///   * read the actual clipboard contents on demand (clipboard handle)
struct ClipboardListener {
    app: AppHandle,
    state: Arc<Mutex<ClipboardState>>,
    service: Arc<anydrop::service::discovery_service::DiscoveryService>,
    config: AnyDropServiceConfig,
    last_event_at: Option<std::time::Instant>,
    clipboard: Option<arboard::Clipboard>,
}

impl ClipboardHandler for ClipboardListener {
    fn on_clipboard_change(&mut self) -> CallbackResult {
        let now = std::time::Instant::now();
        let elapsed_ms = self
            .last_event_at
            .map(|t| now.duration_since(t).as_millis())
            .unwrap_or(u128::MAX);
        let is_double_tap = elapsed_ms <= DOUBLE_COPY_WINDOW_MS;
        self.last_event_at = Some(now);

        let clipboard = match self.clipboard.as_mut() {
            Some(c) => c,
            None => return CallbackResult::Next,
        };

        // Probe text first (more common), fall through to image when text is
        // empty or unavailable. arboard returns Err on the wrong payload
        // type; both branches are normal paths, not real errors.
        let text = clipboard.get_text().ok().filter(|t| !t.is_empty());

        if text.is_none() {
            // No text → try image. If both fail, just bail; user copied
            // something we can't sync (e.g., file URI list).
            return self.try_broadcast_image(is_double_tap);
        }
        let text = text.unwrap();

        let backend = match self.app.try_state::<Backend>() {
            Some(b) => b,
            None => return CallbackResult::Next,
        };
        let settings = backend.settings.lock().unwrap().clone();

        // Suppression catches our own loopback: when a peer's text arrives,
        // we write it to the local clipboard which fires this very event.
        let mut state = self.state.lock().unwrap();
        if state.suppressed_text.as_deref() == Some(text.as_str()) {
            state.suppressed_text = None;
            state.last_text = text;
            return CallbackResult::Next;
        }

        if !settings.send_clipboard_enabled {
            state.last_text = text;
            return CallbackResult::Next;
        }

        let should_send = if settings.send_only_on_double_copy {
            // The OS clipboard sequence ticks even when the user re-copies
            // the same content, so this works for "select+Ctrl+C+Ctrl+C" too.
            is_double_tap
        } else {
            // Default mode: send on every distinct content change. (Two
            // consecutive copies of the same text won't broadcast twice.)
            state.last_text != text
        };

        state.last_text = text.clone();
        if !should_send {
            return CallbackResult::Next;
        }

        // Suppress the broadcast's own loopback before we send.
        state.suppressed_text = Some(text.clone());
        drop(state);

        // Re-arm: require a fresh pair of taps for the next double-copy send.
        if settings.send_only_on_double_copy {
            self.last_event_at = None;
        }

        shared_anydrop_broadcast_text(text, self.service.clone(), &self.config);
        emit_snapshot(&self.app);
        CallbackResult::Next
    }

    fn on_clipboard_error(&mut self, error: std::io::Error) -> CallbackResult {
        eprintln!("clipboard listener error: {error}");
        CallbackResult::Next
    }
}

impl ClipboardListener {
    /// Pull the clipboard image (if any), PNG-encode, broadcast.  Same gates
    /// as text — send_clipboard_enabled, sync_image_enabled, double-copy
    /// option, loopback suppression — just on the image path.
    fn try_broadcast_image(&mut self, is_double_tap: bool) -> CallbackResult {
        let clipboard = match self.clipboard.as_mut() {
            Some(c) => c,
            None => return CallbackResult::Next,
        };
        let img = match clipboard.get_image() {
            Ok(i) => i,
            Err(_) => return CallbackResult::Next,
        };
        let raw_rgba: Vec<u8> = img.bytes.into_owned();
        let width = img.width as u32;
        let height = img.height as u32;
        if width == 0 || height == 0 || raw_rgba.is_empty() {
            return CallbackResult::Next;
        }
        let digest = sha256_short(&raw_rgba);

        // Settings + suppression gates.
        let backend = match self.app.try_state::<Backend>() {
            Some(b) => b,
            None => return CallbackResult::Next,
        };
        let settings = backend.settings.lock().unwrap().clone();

        let mut state = self.state.lock().unwrap();
        if state.suppressed_image_digest == Some(digest) {
            // We just pushed this image to the local clipboard from a peer
            // — don't bounce it back.
            state.suppressed_image_digest = None;
            state.last_image_digest = Some(digest);
            return CallbackResult::Next;
        }
        if !settings.send_clipboard_enabled || !settings.sync_image_enabled {
            state.last_image_digest = Some(digest);
            return CallbackResult::Next;
        }

        let should_send = if settings.send_only_on_double_copy {
            is_double_tap
        } else {
            state.last_image_digest != Some(digest)
        };
        state.last_image_digest = Some(digest);
        if !should_send {
            return CallbackResult::Next;
        }
        // Pre-arm suppression so the post-encode broadcast doesn't loop back.
        state.suppressed_image_digest = Some(digest);
        drop(state);

        if settings.send_only_on_double_copy {
            self.last_event_at = None;
        }

        // Encode RGBA → PNG. PNG buffer comes back ready to wire.
        let mut png_bytes: Vec<u8> = Vec::new();
        let encode_result = {
            use image::codecs::png::PngEncoder;
            use image::ImageEncoder;
            let encoder = PngEncoder::new(&mut png_bytes);
            encoder.write_image(&raw_rgba, width, height, image::ExtendedColorType::Rgba8)
        };
        if let Err(e) = encode_result {
            eprintln!("clipboard image encode failed: {e}");
            return CallbackResult::Next;
        }

        anydrop::lib_util::shared_anydrop_broadcast_image(
            png_bytes,
            self.service.clone(),
            &self.config,
        );
        emit_snapshot(&self.app);
        CallbackResult::Next
    }
}

fn start_runtime(app: &AppHandle, backend: &Backend, settings: AppSettings) -> Result<(), String> {
    stop_runtime(backend);
    *backend.peers.lock().unwrap() = Vec::new();
    port_available(settings.discovery_port, settings.data_port)?;

    let config = AnyDropServiceConfig {
        discovery_service_server_port: settings.discovery_port,
        discovery_service_client_port: 0,
        text_service_listen_addr: "0.0.0.0".to_string(),
        data_service_listen_port: settings.data_port,
        group_identifier: settings.group_identity,
    };
    let service = Arc::new(AnyDropService::new(&config).map_err(|err| err.to_string())?);
    let stop = Arc::new(AtomicBool::new(false));
    let mut threads = Vec::new();

    let discovery_stop = stop.clone();
    let discovery_peers = service.discovery_service().peers();
    let discovery_thread_peers = discovery_peers.clone();
    let peer_last_seen: Arc<Mutex<HashMap<String, std::time::Instant>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let discovery_last_seen = peer_last_seen.clone();
    let polling_last_seen = peer_last_seen.clone();
    let peer_cache = backend.peers.clone();
    let peer_app = app.clone();
    let discovery_config = config.clone();
    let discovery_display_name = settings.display_name.clone();
    let discovery_device_id = settings.device_id.clone();
    threads.push(thread::spawn(move || {
        let _ = DiscoveryService::run(
            discovery_config.discovery_service_client_port,
            discovery_config.discovery_service_server_port,
            discovery_thread_peers,
            Some(discovery_last_seen),
            Box::new(move || discovery_stop.load(Ordering::SeqCst)),
            discovery_config.group_identifier,
            discovery_display_name,
            discovery_device_id,
        );
    }));

    // mDNS / DNS-SD discovery runs alongside the UDP broadcast above and
    // feeds the same peers set + last_seen map (so the existing TTL sweep
    // applies uniformly).  HashSet<Peer> dedups by (host, port), so peers
    // seen on both channels collapse to one entry automatically.
    let mdns_stop = stop.clone();
    let mdns_peers = discovery_peers.clone();
    let mdns_last_seen = peer_last_seen.clone();
    let mdns_data_port = config.data_service_listen_port;
    let mdns_group = config.group_identifier;
    let mdns_display_name = settings.display_name.clone();
    let mdns_device_id = settings.device_id.clone();
    threads.push(thread::spawn(move || {
        if let Err(e) = anydrop::service::mdns_discovery::run(
            mdns_peers,
            Some(mdns_last_seen),
            mdns_data_port,
            mdns_group,
            mdns_display_name,
            mdns_device_id,
            Box::new(move || mdns_stop.load(Ordering::SeqCst)),
        ) {
            eprintln!("mdns discovery error: {e}");
        }
    }));

    // Periodic discovery refresh.  Two pulses per cycle:
    //   1. Broadcast — finds brand-new peers via the standard LAN scan.
    //   2. Unicast to every peer we already know — symmetrizes discovery in
    //      mesh / Wi-Fi-to-Ethernet topologies where broadcast / multicast
    //      only propagates one way.  Without this, a peer reachable from us
    //      by unicast (file send works fine) might never learn we exist
    //      because their broadcast/multicast reaches us but ours doesn't
    //      reach them.
    let rebroadcast_stop = stop.clone();
    let rebroadcast_config = config.clone();
    let rebroadcast_display_name = settings.display_name.clone();
    let rebroadcast_device_id = settings.device_id.clone();
    let rebroadcast_peers = discovery_peers.clone();
    threads.push(thread::spawn(move || {
        let mut tick: u32 = 0;
        while !rebroadcast_stop.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(250));
            tick = tick.wrapping_add(1);
            if tick % 12 != 0 {
                continue;
            }

            let _ = DiscoveryService::broadcast_discovery_request(
                rebroadcast_config.discovery_service_client_port,
                rebroadcast_config.discovery_service_server_port,
                rebroadcast_config.group_identifier,
                &rebroadcast_display_name,
                &rebroadcast_device_id,
            );

            // Snapshot the peer set so we don't hold the lock across unicasts.
            let targets: Vec<Ipv4Addr> = rebroadcast_peers
                .lock()
                .ok()
                .map(|set| {
                    set.iter()
                        .filter_map(|p| p.host().parse::<Ipv4Addr>().ok())
                        .collect()
                })
                .unwrap_or_default();
            // Assume peer uses the same discovery port as us — true for
            // every default-config deployment.  If users start customizing
            // ports per-device, we'd need to advertise it (e.g. mDNS TXT)
            // and read it back, but that's beyond what's worth doing now.
            let discovery_port = rebroadcast_config.discovery_service_server_port;
            for ip in targets {
                let _ = DiscoveryService::unicast_discovery_to(
                    ip,
                    discovery_port,
                    discovery_port,
                    rebroadcast_config.group_identifier,
                    &rebroadcast_display_name,
                    &rebroadcast_device_id,
                );
            }
        }
    }));

    let peer_stop = stop.clone();
    threads.push(thread::spawn(move || {
        const PEER_TTL: Duration = Duration::from_secs(15);
        while !peer_stop.load(Ordering::SeqCst) {
            let now = std::time::Instant::now();

            // Build a deny-list of stale hosts. Peers not in the map at all
            // (race during init / freshly added) are kept, so we never filter
            // out a peer the discovery service just inserted.
            let stale_hosts: std::collections::HashSet<String> = polling_last_seen
                .try_lock()
                .ok()
                .map(|seen| {
                    seen.iter()
                        .filter(|(_, t)| now.duration_since(**t) >= PEER_TTL)
                        .map(|(host, _)| host.clone())
                        .collect()
                })
                .unwrap_or_default();

            // Prune the underlying peer set so stale entries don't linger.
            if !stale_hosts.is_empty() {
                if let Ok(mut peers) = discovery_peers.try_lock() {
                    peers.retain(|p| !stale_hosts.contains(p.host()));
                }
                if let Ok(mut seen) = polling_last_seen.try_lock() {
                    seen.retain(|host, _| !stale_hosts.contains(host));
                }
            }

            if let Ok(peers) = discovery_peers.try_lock() {
                let next = group_peers(peers.iter().cloned());
                drop(peers);
                // Detect change, update cache, then RELEASE the lock before
                // emitting. emit_snapshot → backend.snapshot() calls
                // peer_cache.try_lock(); if we still hold it here the
                // try_lock fails and the snapshot is emitted with an empty
                // peer list, making auto-refresh look broken.
                let changed = {
                    if let Ok(mut cache) = peer_cache.lock() {
                        let differs = cache.len() != next.len()
                            || cache.iter().zip(next.iter()).any(|(left, right)| {
                                left.name != right.name || left.hosts != right.hosts
                            });
                        if differs {
                            *cache = next;
                        }
                        differs
                    } else {
                        false
                    }
                }; // lock released here
                if changed {
                    emit_snapshot(&peer_app);
                }
            }
            thread::sleep(Duration::from_millis(750));
        }
    }));

    let data_stop = stop.clone();
    let data_service = service.clone();
    let data_app = app.clone();
    let clipboard_state = backend.clipboard.clone();
    threads.push(thread::spawn(move || {
        let text_app = data_app.clone();
        let text_clipboard = clipboard_state.clone();
        let text_callback = move |packet: &TextPacket, peer: Option<&Peer>| {
            let text = packet.text().clone();
            let mut popup_enabled = false;
            if let Some(backend) = text_app.try_state::<Backend>() {
                let settings = backend.settings.lock().unwrap();
                if !settings.receive_clipboard_enabled {
                    return;
                }
                popup_enabled = settings.clipboard_popup_enabled;
            }

            if let Ok(mut guard) = text_clipboard.lock() {
                // Drop rapid exact-duplicate deliveries (same text reaching us via
                // two of our own addresses). A genuine re-send of the same text
                // after the window still gets through.
                let now = std::time::Instant::now();
                let is_dup = guard.last_received_text == text
                    && guard
                        .last_received_at
                        .map(|t| now.duration_since(t) < Duration::from_millis(1500))
                        .unwrap_or(false);
                guard.last_received_at = Some(now);
                if is_dup {
                    return;
                }
                guard.last_received_text = text.clone();
                guard.last_text = text.clone();
                guard.suppressed_text = Some(text.clone());
            }
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(text.clone());
            }
            let _ = text_app.emit(
                "text-received",
                serde_json::json!({ "text": text, "peer": peer_text(peer) }),
            );
            // Optional bottom-right popup mirroring the file-receive one.
            if popup_enabled && !popup_suppressed_by_game(&text_app) {
                ensure_receive_window(&text_app);
                let preview: String = text.chars().take(200).collect();
                let _ = text_app.emit(
                    "clipboard-popup",
                    serde_json::json!({
                        "kind": "text",
                        "preview": preview,
                        "text": text,
                        "peer": peer_text(peer),
                    }),
                );
            }
            emit_snapshot(&text_app);
        };

        let image_app = data_app.clone();
        let image_clipboard = clipboard_state.clone();
        let image_callback = move |packet: &anydrop::packet::data::image_packet::ImagePacket,
                                   peer: Option<&Peer>| {
            // Honour the user's image-sync toggle. We deliberately reuse the
            // existing receive_clipboard_enabled gate AND add a finer-grained
            // sync_image_enabled flag so users can have text sync on but
            // images off.
            let mut popup_enabled = false;
            if let Some(backend) = image_app.try_state::<Backend>() {
                let settings = backend.settings.lock().unwrap().clone();
                if !settings.receive_clipboard_enabled || !settings.sync_image_enabled {
                    return;
                }
                popup_enabled = settings.clipboard_popup_enabled;
            }

            // Decode PNG into raw RGBA for arboard.set_image. We're holding
            // the bytes already; decode is just for format conversion.
            let png = packet.png_bytes();
            let img = match image::load_from_memory_with_format(png, image::ImageFormat::Png) {
                Ok(i) => i.to_rgba8(),
                Err(e) => {
                    eprintln!("image_callback: decode failed: {e}");
                    return;
                }
            };
            let (w, h) = img.dimensions();
            let raw = img.into_raw();

            // Compute a hash of the decoded RGBA so the suppression check in
            // the clipboard listener (next event after set_image) can
            // recognize and skip our own write.
            let digest = sha256_short(&raw);
            if let Ok(mut guard) = image_clipboard.lock() {
                guard.suppressed_image_digest = Some(digest);
            }

            if let Ok(mut cb) = arboard::Clipboard::new() {
                let data = arboard::ImageData {
                    width: w as usize,
                    height: h as usize,
                    bytes: std::borrow::Cow::Owned(raw),
                };
                if let Err(e) = cb.set_image(data) {
                    eprintln!("image_callback: set_image failed: {e}");
                }
            }
            let _ = image_app.emit(
                "image-received",
                serde_json::json!({
                    "width": w,
                    "height": h,
                    "bytes": packet.png_bytes().len(),
                    "peer": peer_text(peer),
                }),
            );
            if popup_enabled && !popup_suppressed_by_game(&image_app) {
                ensure_receive_window(&image_app);
                let _ = image_app.emit(
                    "clipboard-popup",
                    serde_json::json!({
                        "kind": "image",
                        "preview": format!("{}×{} 图片", w, h),
                        "peer": peer_text(peer),
                    }),
                );
            }
            emit_snapshot(&image_app);
        };

        // File transfer is owned by the QUIC `transfer` server (see below);
        // the legacy TCP DataService now only carries clipboard payloads
        // (text + images).
        let context = DataServiceContext::new(
            data_service.config().text_service_listen_addr,
            data_service.config().data_service_listen_port,
            Arc::new(Box::new(text_callback)),
            Arc::new(Box::new(image_callback)),
            data_service.discovery_service(),
        );
        let _ = DataService::run(context, Box::new(move || data_stop.load(Ordering::SeqCst)));
    }));

    // Clipboard listener (event-driven).
    //
    // The previous polling implementation could only react to *content*
    // changes, which made the "send on double Ctrl+C" toggle impossible to
    // satisfy — copying the same text twice produced one content change, not
    // two events. `clipboard-master` taps the OS-level clipboard sequence
    // (Win: GetClipboardSequenceNumber, mac: NSPasteboard.changeCount,
    // Linux: X11/Wayland selection events) so we genuinely see every copy.
    let clipboard_app = app.clone();
    let clipboard_stop = stop.clone();
    let clipboard_state = backend.clipboard.clone();
    let clipboard_service = service.discovery_service();
    let clipboard_config = config.clone();
    threads.push(thread::spawn(move || {
        let handler = ClipboardListener {
            app: clipboard_app,
            state: clipboard_state,
            service: clipboard_service,
            config: clipboard_config,
            last_event_at: None,
            clipboard: arboard::Clipboard::new().ok(),
        };
        let mut master = match Master::new(handler) {
            Ok(m) => m,
            Err(err) => {
                eprintln!("clipboard listener init failed: {err}");
                return;
            }
        };
        // Bridge our AtomicBool shutdown flag to clipboard-master's channel.
        let shutdown = master.shutdown_channel();
        let stop_flag = clipboard_stop.clone();
        thread::spawn(move || {
            while !stop_flag.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(250));
            }
            let _ = shutdown.signal();
        });
        if let Err(err) = master.run() {
            eprintln!("clipboard listener stopped: {err}");
        }
    }));

    *backend.runtime.lock().unwrap() = Some(ServiceRuntime {
        service,
        config,
        stop,
        threads,
    });

    // Spin up the QUIC transfer server on the same port number as the legacy
    // TCP text data service (UDP/TCP namespaces don't collide).
    let transfer_bind: SocketAddr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), settings.data_port);
    let offer_app = app.clone();
    let offer_transfers = backend.transfers.clone();
    let offer_log = backend.log_entries.clone();
    let on_offer = move |offer: TransferOffer| {
        add_log(
            &offer_log,
            format!(
                "offer: from='{}' addr={} items={} bytes={}",
                offer.display_name,
                offer.remote_addr,
                offer.items.len(),
                offer.total_size
            ),
        );
        let label = offer_label(&offer);
        let top_level = offer
            .items
            .first()
            .map(|i| top_level_name(&i.rel_path))
            .unwrap_or_default();
        let key = make_transfer_key(offer.transfer_id);
        let t = Transfer {
            key: key.clone(),
            file_id: 0,
            file_name: label,
            remote_path: format!("transfer_id:{}", offer.transfer_id),
            local_path: String::new(),
            peer: format!("{}@{}", offer.display_name, offer.remote_addr),
            host: offer.remote_addr.ip().to_string(),
            direction: "incoming".to_string(),
            progress: 0,
            total: offer.total_size,
            status: 1,
            error: None,
            speed_bps: 0.0,
            speed_last_at: None,
            speed_last_bytes: 0,
            top_level,
            reveal_path: String::new(), // set on accept once save_root is known
        };
        offer_transfers.lock().unwrap().insert(key, t.clone());
        save_transfers(&offer_transfers.lock().unwrap());
        let _ = offer_app.emit("incoming-file", t);
        // Surface the offer natively: bottom-right popup + dock bounce / taskbar
        // flash + a system notification banner. Skipped while a fullscreen game
        // is in front (if the user kept that option on).
        if !popup_suppressed_by_game(&offer_app) {
            ensure_receive_window(&offer_app);
            request_attention(&offer_app);
            let _ = offer_app
                .notification()
                .builder()
                .title("AnyDrop 收到文件请求")
                .body(offer_label(&offer))
                .show();
        }
        emit_snapshot(&offer_app);
    };

    let prog_app = app.clone();
    let prog_transfers = backend.transfers.clone();
    let prog_log = backend.log_entries.clone();
    let on_progress = move |p: TransferProgress| {
        // Log terminal states unconditionally, plus any InProgress event that
        // carries an error string — that's how the client surfaces transient
        // per-attempt failures (used to diagnose stalls / retries).
        let should_log = matches!(
            p.status,
            TransferStatus::AllDone | TransferStatus::Error | TransferStatus::Rejected
        ) || p.error.is_some();
        if should_log {
            add_log(
                &prog_log,
                format!(
                    "{:?}: id={} dir={:?} done={}/{}{}",
                    p.status,
                    p.transfer_id,
                    p.direction,
                    p.total_done,
                    p.total_size,
                    p.error
                        .as_ref()
                        .map(|e| format!(" err={}", e))
                        .unwrap_or_default()
                ),
            );
        }
        let t = apply_progress(&prog_transfers, &p, None);
        let _ = prog_app.emit("transfer-updated", t);
        // Only re-emit the full snapshot on terminal states (or when a log
        // line was added). The high-frequency progress updates already flow
        // to the UI via `transfer-updated`; pumping a full Snapshot
        // serialization through the Tauri IPC on every chunk-progress event
        // dominated CPU + IPC bandwidth in folder transfers with many
        // concurrent files. The transfer-list React component reconciles
        // from `transfer-updated` directly, so the UI stays current
        // regardless.
        if should_log {
            // Terminal / error states change the persistent history — save it.
            save_transfers(&prog_transfers.lock().unwrap());
            emit_snapshot(&prog_app);
        }
    };

    let display_name = settings.display_name.clone();
    // When a peer asks us to resume a transfer it's receiving from us, we are
    // the sender — reconnect via our parked send_args. Look the handle up at
    // call time (it's stored just below, after start_server returns).
    let resume_app = app.clone();
    let on_resume_request = move |transfer_id: u64| {
        if let Some(backend) = resume_app.try_state::<Backend>() {
            let handle = backend.transfer_handle.lock().unwrap().clone();
            if let Some(handle) = handle {
                let restarted = handle.resume_transfer(transfer_id);
                backend.log(format!(
                    "resume-request from peer: id={transfer_id} restarted={restarted}"
                ));
            }
        }
    };
    match transfer::start_server(transfer_bind, display_name, on_offer, on_progress, on_resume_request)
    {
        Ok(handle) => {
            *backend.transfer_handle.lock().unwrap() = Some(Arc::new(handle));
            backend.log(format!("transfer server listening on udp/{}", settings.data_port));
        }
        Err(err) => {
            backend.log(format!("transfer server failed: {}", err));
        }
    }

    backend.set_status(format!("Online on LAN Group #{}", settings.group_identity));
    backend.log(format!(
        "service started (discovery_port={} data_port={} group={})",
        settings.discovery_port, settings.data_port, settings.group_identity
    ));
    Ok(())
}

fn stop_runtime(backend: &Backend) {
    if let Some(runtime) = backend.runtime.lock().unwrap().take() {
        runtime.stop();
    }
    if let Some(handle) = backend.transfer_handle.lock().unwrap().take() {
        handle.close();
    }
}

fn start_service_background(app: AppHandle) {
    thread::spawn(move || {
        let Some(backend) = app.try_state::<Backend>() else {
            return;
        };
        backend.set_status("Starting service");
        emit_snapshot(&app);

        let settings = backend.settings.lock().unwrap().clone();
        match start_runtime(&app, &backend, settings) {
            Ok(()) => emit_snapshot(&app),
            Err(err) => {
                backend.set_status(format!("Service start failed: {err}"));
                emit_snapshot(&app);
            }
        }
    });
}

fn auto_start_service(app: AppHandle) {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(350));
        start_service_background(app);
    });
}

#[tauri::command]
fn get_snapshot(backend: State<'_, Backend>) -> Snapshot {
    backend.snapshot()
}

#[tauri::command]
fn refresh_peers(backend: State<'_, Backend>) -> Result<Snapshot, String> {
    let Some(config) = backend.config() else {
        return Err("Service is offline".to_string());
    };
    // Fire-and-forget: 3 broadcasts with small spacing to reliably reach peers
    // even with UDP packet loss.
    let cfg = config.clone();
    let (display_name, device_id) = {
        let s = backend.settings.lock().unwrap();
        (s.display_name.clone(), s.device_id.clone())
    };
    thread::spawn(move || {
        for _ in 0..3 {
            let _ = DiscoveryService::broadcast_discovery_request(
                cfg.discovery_service_client_port,
                cfg.discovery_service_server_port,
                cfg.group_identifier,
                &display_name,
                &device_id,
            );
            thread::sleep(Duration::from_millis(120));
        }
    });
    Ok(backend.snapshot())
}

#[tauri::command]
fn start_service(app: AppHandle, backend: State<'_, Backend>) -> Result<Snapshot, String> {
    backend.set_status("Starting service");
    start_service_background(app.clone());
    emit_snapshot(&app);
    Ok(backend.snapshot())
}

#[tauri::command]
fn stop_service(app: AppHandle, backend: State<'_, Backend>) -> Snapshot {
    stop_runtime(&backend);
    backend.set_status("Offline");
    emit_snapshot(&app);
    backend.snapshot()
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    backend: State<'_, Backend>,
    settings: AppSettings,
) -> Result<Snapshot, String> {
    let settings = normalize_settings(settings);
    // Only fields baked into the running services at start time need a restart.
    // Clipboard toggles, theme, save dir, and the popup flag are read live (or
    // only matter on the next receive), so flipping them must NOT tear down
    // discovery / transfers — that used to drop peers and in-flight files.
    let old = backend.settings.lock().unwrap().clone();
    let net_changed = old.discovery_port != settings.discovery_port
        || old.data_port != settings.data_port
        || old.group_identity != settings.group_identity
        || old.display_name != settings.display_name;
    if backend.is_running() && net_changed {
        start_runtime(&app, &backend, settings.clone())?;
    }
    save_settings_file(&settings)?;
    *backend.settings.lock().unwrap() = settings;
    emit_snapshot(&app);
    Ok(backend.snapshot())
}

#[tauri::command]
fn send_clipboard_now(app: AppHandle, backend: State<'_, Backend>) -> Result<Snapshot, String> {
    let text = arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.get_text())
        .map_err(|err| err.to_string())?;
    let Some(service) = backend.service() else {
        return Err("Service is offline".to_string());
    };
    let Some(config) = backend.config() else {
        return Err("Service is offline".to_string());
    };
    shared_anydrop_broadcast_text(text.clone(), service.discovery_service(), &config);
    backend.clipboard.lock().unwrap().last_text = text;
    backend.set_status("Clipboard sent to LAN peers");
    emit_snapshot(&app);
    Ok(backend.snapshot())
}

#[tauri::command]
fn send_text_to_peer(
    app: AppHandle,
    backend: State<'_, Backend>,
    hosts: Vec<String>,
    text: String,
) -> Result<Snapshot, String> {
    let Some(config) = backend.config() else {
        return Err("Service is offline".to_string());
    };
    let Some(host) = best_reachable_host(&hosts, config.data_service_listen_port) else {
        return Err("No reachable address for peer".to_string());
    };
    let packet = TextPacket::new(text).map_err(|err| format!("{err}"))?;
    DataService::send_once_with_retry(
        &Peer::new(&host, config.data_service_listen_port, None),
        config.data_service_listen_port,
        MagicNumbers::Text,
        &packet.serialize(),
        Duration::from_millis(CONNECTION_TIMEOUT_MILLIS),
    )
    .map_err(|err| err.to_string())?;
    backend.set_status(format!("Text sent to {host}"));
    emit_snapshot(&app);
    Ok(backend.snapshot())
}

#[tauri::command]
fn send_paths(
    app: AppHandle,
    backend: State<'_, Backend>,
    hosts: Vec<String>,
    paths: Vec<String>,
) -> Result<Snapshot, String> {
    let handle = backend
        .transfer_handle
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "transfer server not running".to_string())?;
    let cfg = backend
        .config()
        .ok_or_else(|| "Service is offline".to_string())?;
    let port = cfg.data_service_listen_port;
    if hosts.is_empty() {
        return Err("no host".to_string());
    }
    let candidates = parse_ipv4_list(&hosts);
    let target = anydrop::util::network::pick_best_peer(
        &candidates,
        port,
        Duration::from_millis(500),
    )
    .ok_or_else(|| "no resolvable host".to_string())?;
    let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    backend.log(format!(
        "send: target={} (from {} candidate(s)) paths={}",
        target,
        candidates.len(),
        path_bufs.len()
    ));
    // Kick off the send and grab the assigned transfer_id so we can
    // pre-populate the Transfer row with a source-folder path. Without this,
    // the sender's transfer never has localPath set and the "open folder"
    // button (which keys off localPath being non-empty) never appears.
    let transfer_id = handle.send_paths(target, path_bufs.clone());

    // local_path = the directory housing the first source path. For folder
    // sends that's the chosen folder itself; for file sends that's the
    // parent of the file. "Open folder" should land the user where their
    // selection started.
    let source_dir = path_bufs
        .first()
        .map(|p| {
            if p.is_dir() {
                p.clone()
            } else {
                p.parent().map(|x| x.to_path_buf()).unwrap_or_else(|| p.clone())
            }
        })
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    // The send-side reveal target is the source path the user picked (the file
    // itself, or the folder). "Open folder" should select that exact item.
    let reveal_path = path_bufs
        .first()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let top_level = path_bufs
        .first()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_default();
    {
        let key = make_transfer_key(transfer_id);
        let mut map = backend.transfers.lock().unwrap();
        // Insert a placeholder so the row's `local_path` is set well before
        // the first apply_progress fires (which preserves it via the
        // or_insert_with → existing branch).
        map.entry(key.clone()).or_insert_with(|| Transfer {
            key,
            file_id: 0,
            file_name: String::new(), // overwritten by the client's summary event
            remote_path: format!("transfer_id:{}", transfer_id),
            local_path: source_dir,
            peer: target.to_string(),
            host: target.ip().to_string(),
            direction: "outgoing".to_string(),
            progress: 0,
            total: 0,
            status: 10, // awaiting peer accept/reject (no bytes yet)
            error: None,
            speed_bps: 0.0,
            speed_last_at: None,
            speed_last_bytes: 0,
            top_level,
            reveal_path,
        });
    }

    backend.persist_transfers();
    backend.set_status(format!("Sending {} item(s) via QUIC", paths.len()));
    emit_snapshot(&app);
    Ok(backend.snapshot())
}

fn parse_transfer_key(key: &str) -> Result<u64, String> {
    key.parse::<u64>()
        .map_err(|_| format!("not a transfer key: {}", key))
}

#[tauri::command]
fn accept_transfer(
    app: AppHandle,
    backend: State<'_, Backend>,
    transfer_key: String,
    save_dir: Option<String>,
) -> Result<Snapshot, String> {
    let transfer_id = parse_transfer_key(&transfer_key)?;
    let handle = backend
        .transfer_handle
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "transfer server not running".to_string())?;
    // Per-transfer override wins; otherwise the configured default; otherwise
    // the hard fallback. Never an empty path.
    let save_root = save_dir
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let cfg = backend.settings.lock().unwrap().default_save_dir.clone();
            if cfg.trim().is_empty() {
                default_save_root()
            } else {
                PathBuf::from(cfg)
            }
        });
    fs::create_dir_all(&save_root).map_err(|err| err.to_string())?;
    backend.log(format!(
        "accept: id={transfer_id} save_root={}",
        save_root.display()
    ));
    if let Some(t) = backend.transfers.lock().unwrap().get_mut(&transfer_key) {
        t.status = 4;
        t.local_path = save_root.to_string_lossy().to_string();
        // Reveal target = the top-level entry under the save root. Falls back
        // to the save root itself if we never learned a top-level name.
        let reveal = if t.top_level.is_empty() {
            save_root.clone()
        } else {
            save_root.join(&t.top_level)
        };
        t.reveal_path = reveal.to_string_lossy().to_string();
    }
    handle.respond(transfer_id, Decision::Accept { save_root });
    backend.persist_transfers();
    emit_snapshot(&app);
    Ok(backend.snapshot())
}

#[tauri::command]
fn reject_transfer(
    app: AppHandle,
    backend: State<'_, Backend>,
    transfer_key: String,
) -> Result<Snapshot, String> {
    let transfer_id = parse_transfer_key(&transfer_key)?;
    let handle = backend
        .transfer_handle
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "transfer server not running".to_string())?;
    backend.log(format!("reject: id={transfer_id}"));
    handle.respond(
        transfer_id,
        Decision::Reject {
            reason: "user rejected".to_string(),
        },
    );
    backend.transfers.lock().unwrap().remove(&transfer_key);
    backend.persist_transfers();
    emit_snapshot(&app);
    Ok(backend.snapshot())
}

/// Cancel an in-flight transfer. Terminal — no resume. Both sender- and
/// receiver-side cancellations route through the same `cancel_transfer` API
/// on `ServerHandle`; whichever role the local node plays for the given
/// transfer_id is the one that gets stopped.
#[tauri::command]
fn cancel_transfer(
    app: AppHandle,
    backend: State<'_, Backend>,
    transfer_key: String,
) -> Result<Snapshot, String> {
    let transfer_id = parse_transfer_key(&transfer_key)?;
    let handle = backend
        .transfer_handle
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "transfer server not running".to_string())?;
    let signalled = handle.cancel_transfer(transfer_id);
    backend.log(format!(
        "cancel: id={transfer_id} signalled={signalled}"
    ));
    // Optimistically mark the row Cancelled in case the backend hasn't yet
    // emitted its terminal Cancelled progress event — UI feedback is instant.
    if let Some(t) = backend.transfers.lock().unwrap().get_mut(&transfer_key) {
        t.status = 5;
        t.speed_bps = 0.0;
    }
    emit_snapshot(&app);
    Ok(backend.snapshot())
}

#[tauri::command]
fn pause_transfer(
    app: AppHandle,
    backend: State<'_, Backend>,
    transfer_key: String,
) -> Result<Snapshot, String> {
    let transfer_id = parse_transfer_key(&transfer_key)?;
    let handle = backend
        .transfer_handle
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "transfer server not running".to_string())?;
    let signalled = handle.pause_transfer(transfer_id);
    backend.log(format!("pause: id={transfer_id} signalled={signalled}"));
    if let Some(t) = backend.transfers.lock().unwrap().get_mut(&transfer_key) {
        t.status = 9;
        t.speed_bps = 0.0;
    }
    emit_snapshot(&app);
    Ok(backend.snapshot())
}

#[tauri::command]
fn resume_transfer(
    app: AppHandle,
    backend: State<'_, Backend>,
    transfer_key: String,
) -> Result<Snapshot, String> {
    let transfer_id = parse_transfer_key(&transfer_key)?;
    let handle = backend
        .transfer_handle
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "transfer server not running".to_string())?;
    let transfer = backend.transfers.lock().unwrap().get(&transfer_key).cloned();
    let is_incoming = transfer
        .as_ref()
        .map(|t| t.direction == "incoming")
        .unwrap_or(false);

    if is_incoming {
        // Receiver side: we can't reconnect (the sender is the QUIC client).
        // Signal the sender to resume; it reconnects and continues from the
        // offsets we still hold.
        let port = backend
            .config()
            .map(|c| c.data_service_listen_port)
            .unwrap_or(DEFAULT_DATA_PORT);
        let host = transfer.as_ref().map(|t| t.host.clone()).unwrap_or_default();
        match host.parse::<Ipv4Addr>() {
            Ok(ip) => {
                handle.request_remote_resume(SocketAddr::new(ip.into(), port), transfer_id);
                backend.log(format!("resume: id={transfer_id} (incoming → signalled {host})"));
                if let Some(t) = backend.transfers.lock().unwrap().get_mut(&transfer_key) {
                    t.status = 4;
                    t.error = None;
                }
            }
            Err(_) => {
                backend.log(format!("resume: id={transfer_id} no sender address ({host})"));
            }
        }
    } else {
        let restarted = handle.resume_transfer(transfer_id);
        backend.log(format!("resume: id={transfer_id} restarted={restarted}"));
        if restarted {
            if let Some(t) = backend.transfers.lock().unwrap().get_mut(&transfer_key) {
                t.status = 4;
                t.error = None;
            }
        }
    }
    emit_snapshot(&app);
    Ok(backend.snapshot())
}

#[tauri::command]
fn clear_logs(app: AppHandle, backend: State<'_, Backend>) -> Snapshot {
    if let Ok(mut log) = backend.log_entries.lock() {
        log.clear();
    }
    emit_snapshot(&app);
    backend.snapshot()
}

/// Copy a received clipboard text back onto the local clipboard (used by the
/// "复制" button on older clipboard receipts). Pre-arms loopback suppression so
/// the clipboard listener doesn't rebroadcast it to peers.
#[tauri::command]
fn copy_text(backend: State<'_, Backend>, text: String) -> Result<(), String> {
    if let Ok(mut guard) = backend.clipboard.lock() {
        guard.suppressed_text = Some(text.clone());
        guard.last_text = text.clone();
        guard.last_received_text = text.clone();
        guard.last_received_at = Some(std::time::Instant::now());
    }
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text))
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn dismiss_transfer(app: AppHandle, backend: State<'_, Backend>, transfer_key: String) -> Snapshot {
    backend.transfers.lock().unwrap().remove(&transfer_key);
    backend.persist_transfers();
    emit_snapshot(&app);
    backend.snapshot()
}

/// Clear all transfer records that aren't currently active (keep in-flight /
/// paused / pending so a running transfer isn't wiped from under the user).
#[tauri::command]
fn clear_transfers(app: AppHandle, backend: State<'_, Backend>) -> Snapshot {
    backend
        .transfers
        .lock()
        .unwrap()
        .retain(|_, t| matches!(t.status, 1 | 4 | 9 | 10));
    backend.persist_transfers();
    emit_snapshot(&app);
    backend.snapshot()
}

#[tauri::command]
fn open_transfer_folder(
    app: AppHandle,
    backend: State<'_, Backend>,
    transfer_key: String,
) -> Result<(), String> {
    let transfer = backend
        .transfers
        .lock()
        .unwrap()
        .get(&transfer_key)
        .cloned()
        .ok_or_else(|| "Transfer not found".to_string())?;
    // Prefer revealing (and selecting) the actual landed file/folder. This
    // fixes the old bug where we opened the *parent* of the save root (e.g.
    // ~/Downloads instead of ~/Downloads/AnyDrop) and selected nothing.
    let target = if !transfer.reveal_path.is_empty() {
        transfer.reveal_path.clone()
    } else if !transfer.local_path.is_empty() {
        transfer.local_path.clone()
    } else {
        return Err("Transfer has no local path".to_string());
    };
    app.opener()
        .reveal_item_in_dir(&target)
        .map_err(|err| err.to_string())
}

/// Open a directory (e.g. the default save folder) in the OS file manager.
#[tauri::command]
fn open_directory(path: String) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("目录为空".to_string());
    }
    // Create it if it doesn't exist yet (nothing received there so far) so the
    // open doesn't just fail.
    let _ = fs::create_dir_all(&path);
    open::that(&path).map_err(|err| err.to_string())
}

/// Show the native window system menu (Restore/Move/Size/Minimize/Maximize/
/// Close) at the cursor — the right-click menu a real titlebar would have.
/// Our window is frameless, so we surface it ourselves via Win32. Windows-only.
#[cfg(windows)]
#[tauri::command]
fn show_window_menu(app: AppHandle) {
    let app_main = app.clone();
    // Must run on the UI thread; TrackPopupMenu is modal and pumps messages.
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::{HWND, POINT};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetCursorPos, GetSystemMenu, PostMessageW, TrackPopupMenu, TPM_RETURNCMD,
            TPM_RIGHTBUTTON, WM_SYSCOMMAND,
        };
        let Some(window) = app_main.get_webview_window("main") else {
            return;
        };
        let Ok(handle) = window.hwnd() else {
            return;
        };
        let hwnd: HWND = handle.0;
        unsafe {
            let menu = GetSystemMenu(hwnd, 0);
            if menu.is_null() {
                return;
            }
            let mut pt = POINT { x: 0, y: 0 };
            if GetCursorPos(&mut pt) == 0 {
                return;
            }
            // TPM_RETURNCMD: return the chosen command instead of posting it,
            // so we can forward it as WM_SYSCOMMAND ourselves.
            let cmd = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                pt.x,
                pt.y,
                0,
                hwnd,
                std::ptr::null(),
            );
            if cmd != 0 {
                PostMessageW(hwnd, WM_SYSCOMMAND, cmd as usize, 0);
            }
        }
    });
}

#[cfg(not(windows))]
#[tauri::command]
fn show_window_menu() {}

#[tauri::command]
fn set_peer_remark(
    app: AppHandle,
    backend: State<'_, Backend>,
    peer_key: String,
    remark: String,
) -> Result<Snapshot, String> {
    {
        let mut remarks = backend.peer_remarks.lock().unwrap();
        let trimmed = remark.trim();
        if trimmed.is_empty() {
            remarks.remove(&peer_key);
        } else {
            remarks.insert(peer_key.clone(), trimmed.to_string());
        }
        save_peer_remarks_file(&remarks)?;
    }
    emit_snapshot(&app);
    Ok(backend.snapshot())
}

#[tauri::command]
fn preview_file(
    app: AppHandle,
    backend: State<'_, Backend>,
    transfer_key: String,
) -> Result<(), String> {
    let transfer = backend
        .transfers
        .lock()
        .unwrap()
        .get(&transfer_key)
        .cloned()
        .ok_or_else(|| "Transfer not found".to_string())?;
    let path = transfer.reveal_path.clone();
    if path.is_empty() {
        return Err("没有可预览的文件".to_string());
    }
    if !Path::new(&path).is_file() {
        return Err("仅支持预览单个文件".to_string());
    }
    let kind = preview_kind(&path);
    // Only image / audio / video are previewable. Everything else (archives,
    // docs, …) gets no preview window at all.
    if kind == "other" {
        return Err("此格式不支持预览".to_string());
    }
    open_preview_window(&app, &path, kind, &transfer.file_name)
}

#[tauri::command]
fn get_preview_payload(backend: State<'_, Backend>) -> Option<serde_json::Value> {
    backend.preview_payload.lock().unwrap().clone()
}

/// True if a fullscreen app (exclusive D3D, presentation mode, or a borderless
/// window covering the whole monitor) is in the foreground. Used to suppress
/// popups/notifications while gaming. Windows-only; a no-op elsewhere.
#[cfg(windows)]
fn is_fullscreen_app_active() -> bool {
    use windows_sys::Win32::UI::Shell::{
        SHQueryUserNotificationState, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
    };
    // 1) The OS's own "don't disturb" signal: D3D exclusive fullscreen or a
    //    presentation app. This is what Windows uses to gate toast popups.
    let mut state = 0i32;
    if unsafe { SHQueryUserNotificationState(&mut state) } == 0
        && (state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_PRESENTATION_MODE)
    {
        return true;
    }
    // 2) Borderless fullscreen: the foreground window covers the ENTIRE monitor
    //    (rcMonitor — not the work area, so a merely maximized window, which
    //    stops above the taskbar, does not count).
    unsafe {
        use windows_sys::Win32::Foundation::RECT;
        use windows_sys::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetDesktopWindow, GetForegroundWindow, GetShellWindow, GetWindowRect,
        };
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() || hwnd == GetDesktopWindow() || hwnd == GetShellWindow() {
            return false;
        }
        let mut wr = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(hwnd, &mut wr) == 0 {
            return false;
        }
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if hmon.is_null() {
            return false;
        }
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(hmon, &mut mi) == 0 {
            return false;
        }
        let m = mi.rcMonitor;
        wr.left <= m.left && wr.top <= m.top && wr.right >= m.right && wr.bottom >= m.bottom
    }
}

#[cfg(not(windows))]
fn is_fullscreen_app_active() -> bool {
    false
}

/// Whether to suppress popups/notifications right now because the user is in a
/// fullscreen game and kept the "don't disturb while gaming" option on.
fn popup_suppressed_by_game(app: &AppHandle) -> bool {
    let enabled = app
        .try_state::<Backend>()
        .map(|b| b.settings.lock().unwrap().suppress_popup_in_game)
        .unwrap_or(true);
    enabled && is_fullscreen_app_active()
}

/// Flash the taskbar (Windows) / bounce the dock (macOS) so the user notices an
/// incoming transfer even when AnyDrop isn't focused. Marshalled to the main
/// thread — called from background network threads, and the underlying Win32
/// call (FlashWindowEx) is unsafe off the UI thread.
fn request_attention(app: &AppHandle) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.request_user_attention(Some(UserAttentionType::Critical));
        }
    });
}

/// Build the receive popup hidden + positioned. Must run on the main thread.
fn make_receive_window(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    match WebviewWindowBuilder::new(
        app,
        RECEIVE_WINDOW_LABEL,
        WebviewUrl::App("index.html?window=receive".into()),
    )
    .title("AnyDrop")
    .inner_size(RECEIVE_WINDOW_WIDTH, RECEIVE_WINDOW_HEIGHT)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .build()
    {
        Ok(window) => {
            position_bottom_right(&window);
            Some(window)
        }
        Err(err) => {
            eprintln!("receive window build failed: {err}");
            None
        }
    }
}

/// Build the receive popup hidden ahead of time (at startup, on the main
/// thread) so the first offer / clipboard receipt appears instantly and never
/// misses the event — the webview's listeners are already registered, and we
/// only ever hide/show it rather than create/destroy.
fn preinit_receive_window(app: &AppHandle) {
    if app.get_webview_window(RECEIVE_WINDOW_LABEL).is_none() {
        let _ = make_receive_window(app);
    }
}

/// Show the (pre-created, persistent) receive popup. Reused for file offers and
/// clipboard receipts; the webview hides it again once it has nothing to show.
///
/// ALL window ops are marshalled to the main thread: this is invoked from
/// background network threads (clipboard / offer callbacks), and showing /
/// focusing a window off the UI thread on Windows can hard-crash the process.
/// We intentionally do NOT steal focus — `always_on_top` makes it visible
/// without yanking focus away from whatever the user is doing.
fn ensure_receive_window(app: &AppHandle) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        if let Some(window) = app.get_webview_window(RECEIVE_WINDOW_LABEL) {
            let _ = window.show();
            return;
        }
        if let Some(window) = make_receive_window(&app) {
            let _ = window.show();
        }
    });
}

/// Dock the popup to the bottom-right of the monitor it landed on. Tauri's
/// monitor API has no work-area accessor, so we reserve a fixed taskbar/dock
/// allowance to avoid sitting underneath it.
fn position_bottom_right(window: &tauri::WebviewWindow) {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let scale = monitor.scale_factor();
    let m_pos = monitor.position();
    let m_size = monitor.size();
    let margin = (POPUP_MARGIN * scale) as i32;
    let taskbar = (48.0 * scale) as i32; // clear a typical taskbar / dock
    let win_w = (RECEIVE_WINDOW_WIDTH * scale) as i32;
    let win_h = (RECEIVE_WINDOW_HEIGHT * scale) as i32;
    let x = m_pos.x + m_size.width as i32 - win_w - margin;
    let y = m_pos.y + m_size.height as i32 - win_h - margin - taskbar;
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// Classify a file for the preview window by extension. Archives intentionally
/// fall through to "other" — we do not preview compressed files.
fn preview_kind(path: &str) -> &'static str {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "ico" | "avif" => "image",
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "oga" | "m4a" | "opus" => "audio",
        "mp4" | "webm" | "mov" | "m4v" | "ogv" => "video",
        _ => "other",
    }
}

/// Create or reuse the Quick-Look style preview window and point it at `path`.
/// All window ops run on the main thread (this is called from a command worker
/// thread; off-UI-thread window calls can crash on Windows).
fn open_preview_window(app: &AppHandle, path: &str, kind: &str, name: &str) -> Result<(), String> {
    let payload = serde_json::json!({ "path": path, "kind": kind, "name": name });
    if let Some(backend) = app.try_state::<Backend>() {
        *backend.preview_payload.lock().unwrap() = Some(payload.clone());
    }
    let app_main = app.clone();
    let title = name.to_string();
    app.run_on_main_thread(move || {
        if let Some(window) = app_main.get_webview_window(PREVIEW_WINDOW_LABEL) {
            let _ = window.emit("preview-load", &payload);
            let _ = window.set_title(&title);
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
            return;
        }
        if let Err(err) = WebviewWindowBuilder::new(
            &app_main,
            PREVIEW_WINDOW_LABEL,
            WebviewUrl::App("index.html?window=preview".into()),
        )
        .title(&title)
        .inner_size(760.0, 580.0)
        .min_inner_size(360.0, 280.0)
        .decorations(false)
        .center()
        .build()
        {
            eprintln!("preview window build failed: {err}");
        }
    })
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn toggle_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let visible = window.is_visible().unwrap_or(false);
        if visible {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "显示 AnyDrop", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    let icon = app
        .default_window_icon()
        .cloned()
        .expect("bundle missing default window icon");

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("AnyDrop")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn run() {
    shared_anydrop_init();
    let mut builder = tauri::Builder::default();
    // Single-instance MUST be the first plugin registered. A second launch
    // re-focuses the existing window instead of spawning another process —
    // which also stops the duplicate tray icons that multiple processes bred.
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(Backend::new(load_settings()))
        .on_window_event(|window, event| {
            // Keep the main window and the persistent receive popup alive across
            // "close" — hide instead of destroy so they reopen instantly.
            if matches!(window.label(), "main" | RECEIVE_WINDOW_LABEL) {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .setup(|app| {
            let handle = app.handle().clone();
            if let Some(backend) = handle.try_state::<Backend>() {
                backend.set_status("Opening UI");
                emit_snapshot(&handle);
            }
            if let Err(err) = build_tray(&handle) {
                eprintln!("tray setup failed: {err}");
            }
            // Pre-create the receive popup (hidden) so the first offer / clipboard
            // receipt shows instantly without an open-then-vanish flash.
            preinit_receive_window(&handle);
            auto_start_service(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            refresh_peers,
            start_service,
            stop_service,
            save_settings,
            send_clipboard_now,
            send_text_to_peer,
            accept_transfer,
            reject_transfer,
            dismiss_transfer,
            open_transfer_folder,
            clear_logs,
            send_paths,
            cancel_transfer,
            pause_transfer,
            resume_transfer,
            set_peer_remark,
            preview_file,
            get_preview_payload,
            copy_text,
            open_directory,
            clear_transfers,
            show_window_menu
        ])
        .run(tauri::generate_context!())
        .expect("error while running AnyDrop");
}
