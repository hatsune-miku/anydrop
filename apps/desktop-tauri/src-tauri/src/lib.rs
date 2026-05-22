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
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

const DEFAULT_DISCOVERY_PORT: u16 = 9818;
const DEFAULT_DATA_PORT: u16 = 9819;

fn settings_path() -> Result<PathBuf, String> {
    let base = dirs::config_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("AnyDrop");
    fs::create_dir_all(&base).map_err(|err| err.to_string())?;
    Ok(base.join("settings.json"))
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
    settings
}

fn load_settings() -> AppSettings {
    let Ok(path) = settings_path() else {
        return normalize_settings(AppSettings::default());
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return normalize_settings(AppSettings::default());
    };
    serde_json::from_str::<AppSettings>(&raw)
        .map(normalize_settings)
        .unwrap_or_else(|_| normalize_settings(AppSettings::default()))
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
        }
    }
}

#[derive(Clone, SerdeSerialize)]
#[serde(rename_all = "camelCase")]
struct PeerGroup {
    label: String,
    name: String,
    hosts: Vec<String>,
}

#[derive(Clone, SerdeSerialize)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
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
}

impl Default for ClipboardState {
    fn default() -> Self {
        Self {
            last_text: String::new(),
            last_received_text: String::new(),
            suppressed_text: None,
        }
    }
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
    /// QUIC transfer server. Started alongside the legacy text data service
    /// when the runtime spins up.
    transfer_handle: Mutex<Option<Arc<TransferServerHandle>>>,
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
    });
    entry.progress = p.total_done;
    if p.total_size > 0 {
        entry.total = p.total_size;
    }
    entry.status = transfer_status_to_u8(p.status);
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
        let peers = self
            .peers
            .try_lock()
            .map(|peers| peers.clone())
            .unwrap_or_default();
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
    let mut by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for peer in peers {
        let host = peer.host().clone();
        let mut name = peer.host_name().clone();
        if name.is_empty() || name == "<empty>" {
            name = host.clone();
        }
        let hosts = by_name.entry(name).or_default();
        if !hosts.contains(&host) {
            hosts.push(host);
        }
    }

    by_name
        .into_iter()
        .map(|(name, mut hosts)| {
            hosts.sort();
            PeerGroup {
                label: format!("{name} ({}个地址)", hosts.len()),
                name,
                hosts,
            }
        })
        .collect()
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

        // Read the current clipboard text. arboard returns Err on non-text
        // payloads (images, files) — silently ignore those.
        let clipboard = match self.clipboard.as_mut() {
            Some(c) => c,
            None => return CallbackResult::Next,
        };
        let text = match clipboard.get_text() {
            Ok(t) => t,
            Err(_) => return CallbackResult::Next,
        };
        if text.is_empty() {
            return CallbackResult::Next;
        }

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
    threads.push(thread::spawn(move || {
        let _ = DiscoveryService::run(
            discovery_config.discovery_service_client_port,
            discovery_config.discovery_service_server_port,
            discovery_thread_peers,
            Some(discovery_last_seen),
            Box::new(move || discovery_stop.load(Ordering::SeqCst)),
            discovery_config.group_identifier,
            discovery_display_name,
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
    threads.push(thread::spawn(move || {
        if let Err(e) = anydrop::service::mdns_discovery::run(
            mdns_peers,
            Some(mdns_last_seen),
            mdns_data_port,
            mdns_group,
            mdns_display_name,
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
            if let Some(backend) = text_app.try_state::<Backend>() {
                if !backend.settings.lock().unwrap().receive_clipboard_enabled {
                    return;
                }
            }

            if let Ok(mut guard) = text_clipboard.lock() {
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
            emit_snapshot(&text_app);
        };

        // File transfer is owned by the QUIC `transfer` server (see below);
        // the legacy TCP DataService now only carries clipboard text.
        let context = DataServiceContext::new(
            data_service.config().text_service_listen_addr,
            data_service.config().data_service_listen_port,
            Arc::new(Box::new(text_callback)),
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
        };
        offer_transfers.lock().unwrap().insert(key, t.clone());
        let _ = offer_app.emit("incoming-file", t);
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
            emit_snapshot(&prog_app);
        }
    };

    let display_name = settings.display_name.clone();
    match transfer::start_server(transfer_bind, display_name, on_offer, on_progress) {
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
    let display_name = backend.settings.lock().unwrap().display_name.clone();
    thread::spawn(move || {
        for _ in 0..3 {
            let _ = DiscoveryService::broadcast_discovery_request(
                cfg.discovery_service_client_port,
                cfg.discovery_service_server_port,
                cfg.group_identifier,
                &display_name,
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
    let should_restart = backend.is_running();
    let settings = normalize_settings(settings);
    if should_restart {
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
    handle.send_paths(target, path_bufs);
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
) -> Result<Snapshot, String> {
    let transfer_id = parse_transfer_key(&transfer_key)?;
    let handle = backend
        .transfer_handle
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "transfer server not running".to_string())?;
    let save_root = dirs::download_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("AnyDrop");
    fs::create_dir_all(&save_root).map_err(|err| err.to_string())?;
    backend.log(format!(
        "accept: id={transfer_id} save_root={}",
        save_root.display()
    ));
    if let Some(t) = backend.transfers.lock().unwrap().get_mut(&transfer_key) {
        t.status = 4;
        t.local_path = save_root.to_string_lossy().to_string();
    }
    handle.respond(transfer_id, Decision::Accept { save_root });
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

#[tauri::command]
fn dismiss_transfer(app: AppHandle, backend: State<'_, Backend>, transfer_key: String) -> Snapshot {
    backend.transfers.lock().unwrap().remove(&transfer_key);
    emit_snapshot(&app);
    backend.snapshot()
}

#[tauri::command]
fn open_transfer_folder(backend: State<'_, Backend>, transfer_key: String) -> Result<(), String> {
    let transfer = backend
        .transfers
        .lock()
        .unwrap()
        .get(&transfer_key)
        .cloned()
        .ok_or_else(|| "Transfer not found".to_string())?;
    let folder = Path::new(&transfer.local_path)
        .parent()
        .ok_or_else(|| "Transfer has no local folder".to_string())?;
    open::that(folder).map_err(|err| err.to_string())
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
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Backend::new(load_settings()))
        .on_window_event(|window, event| {
            if window.label() == "main" {
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
            send_paths
        ])
        .run(tauri::generate_context!())
        .expect("error while running AnyDrop");
}
