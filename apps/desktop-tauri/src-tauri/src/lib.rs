use anydrop::lib_util::{
    shared_anydrop_broadcast_text, shared_anydrop_init, shared_anydrop_respond_to_file,
    shared_anydrop_try_send_file, CONNECTION_TIMEOUT_MILLIS,
};
use anydrop::network::peer::Peer;
use anydrop::packet::data::file_coming_packet::FileComingPacket;
use anydrop::packet::data::file_part_packet::FilePartPacket;
use anydrop::packet::data::local::file_sending_packet::FileSendingPacket;
use anydrop::packet::data::magic_numbers::MagicNumbers;
use anydrop::packet::data::text_packet::TextPacket;
use anydrop::packet::protocol::serialize::Serialize;
use anydrop::service::anydrop_service::{AnyDropService, AnyDropServiceConfig};
use anydrop::service::context::data_service_context::DataServiceContext;
use anydrop::service::data_service::DataService;
use anydrop::service::discovery_service::DiscoveryService;
use serde::{Deserialize, Serialize as SerdeSerialize};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
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
    settings
}

fn load_settings() -> AppSettings {
    let Ok(path) = settings_path() else {
        return AppSettings::default();
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return AppSettings::default();
    };
    serde_json::from_str::<AppSettings>(&raw)
        .map(normalize_settings)
        .unwrap_or_default()
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
}

struct ReceivingFile {
    path: PathBuf,
    total: u64,
    progress: u64,
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
    receiving: Arc<Mutex<HashMap<u8, ReceivingFile>>>,
    clipboard: Arc<Mutex<ClipboardState>>,
    next_file_id: AtomicU8,
    status_text: Mutex<String>,
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

        Snapshot {
            running: self.is_running(),
            settings,
            peers,
            transfers: self.transfer_list(),
            last_clipboard_text,
            last_received_text,
            status_text,
        }
    }

    fn set_status(&self, text: impl Into<String>) {
        *self.status_text.lock().unwrap() = text.into();
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

fn incoming_key(file_id: u8) -> String {
    format!("incoming:{file_id}")
}

fn outgoing_key(file_id: u8) -> String {
    format!("outgoing:{file_id}")
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("received-file")
        .to_string()
}

fn local_receive_path(remote_path: &str) -> Result<PathBuf, String> {
    let base = dirs::download_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("AnyDrop");
    fs::create_dir_all(&base).map_err(|err| err.to_string())?;

    let name = file_name(remote_path);
    let candidate = base.join(&name);
    if !candidate.exists() {
        return Ok(candidate);
    }

    let path = Path::new(&name);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("received-file");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    for index in 1..10_000 {
        let next = if ext.is_empty() {
            base.join(format!("{stem} ({index})"))
        } else {
            base.join(format!("{stem} ({index}).{ext}"))
        };
        if !next.exists() {
            return Ok(next);
        }
    }

    Ok(base.join(format!("received-file-{}", chrono_like_timestamp())))
}

fn chrono_like_timestamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn peer_text(peer: Option<&Peer>) -> String {
    peer.map(ToString::to_string)
        .unwrap_or_else(|| Peer::default().to_string())
}

fn parse_peer_host(peer: &str) -> String {
    let Some(at) = peer.find('@') else {
        return peer.to_string();
    };
    let Some(colon) = peer.rfind(':') else {
        return peer.to_string();
    };
    if colon > at {
        peer[at + 1..colon].to_string()
    } else {
        peer.to_string()
    }
}

fn port_available(discovery_port: u16, data_port: u16) -> Result<(), String> {
    UdpSocket::bind((Ipv4Addr::UNSPECIFIED, discovery_port))
        .map_err(|err| format!("Discovery port {discovery_port} is unavailable: {err}"))?;
    TcpListener::bind((Ipv4Addr::UNSPECIFIED, data_port))
        .map_err(|err| format!("Data port {data_port} is unavailable: {err}"))?;
    Ok(())
}

fn first_reachable_host(hosts: &[String], data_port: u16) -> Option<String> {
    hosts.iter().find_map(|host| {
        let socket = format!("{host}:{data_port}").parse::<SocketAddr>().ok()?;
        TcpStream::connect_timeout(&socket, Duration::from_millis(250))
            .ok()
            .map(|_| host.clone())
    })
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
    let peer_cache = backend.peers.clone();
    let peer_app = app.clone();
    let discovery_config = config.clone();
    threads.push(thread::spawn(move || {
        let _ = DiscoveryService::run(
            discovery_config.discovery_service_client_port,
            discovery_config.discovery_service_server_port,
            discovery_thread_peers,
            Box::new(move || discovery_stop.load(Ordering::SeqCst)),
            discovery_config.group_identifier,
        );
    }));

    // Periodic discovery rebroadcast so new/restarted peers are seen.
    let rebroadcast_stop = stop.clone();
    let rebroadcast_config = config.clone();
    threads.push(thread::spawn(move || {
        // Heartbeat broadcast every 5s, with a short sleep granularity so
        // stopping the service is responsive.
        let mut tick: u32 = 0;
        while !rebroadcast_stop.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(250));
            tick = tick.wrapping_add(1);
            if tick % 20 == 0 {
                let _ = DiscoveryService::broadcast_discovery_request(
                    rebroadcast_config.discovery_service_client_port,
                    rebroadcast_config.discovery_service_server_port,
                    rebroadcast_config.group_identifier,
                );
            }
        }
    }));

    let peer_stop = stop.clone();
    threads.push(thread::spawn(move || {
        while !peer_stop.load(Ordering::SeqCst) {
            if let Ok(peers) = discovery_peers.try_lock() {
                let next = group_peers(peers.iter().cloned());
                drop(peers);
                if let Ok(mut cache) = peer_cache.lock() {
                    if cache.len() != next.len()
                        || cache.iter().zip(next.iter()).any(|(left, right)| {
                            left.name != right.name || left.hosts != right.hosts
                        })
                    {
                        *cache = next;
                        emit_snapshot(&peer_app);
                    }
                }
            }
            thread::sleep(Duration::from_millis(750));
        }
    }));

    let data_stop = stop.clone();
    let data_service = service.clone();
    let data_app = app.clone();
    let transfers = backend.transfers.clone();
    let receiving = backend.receiving.clone();
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

        let file_app = data_app.clone();
        let file_transfers = transfers.clone();
        let file_callback = move |packet: &FileComingPacket, peer: Option<&Peer>| {
            let file_id = file_app
                .try_state::<Backend>()
                .map(|backend| backend.next_file_id.fetch_add(1, Ordering::SeqCst))
                .unwrap_or(0);
            let peer_label = peer_text(peer);
            let transfer = Transfer {
                key: incoming_key(file_id),
                file_id,
                file_name: file_name(packet.file_name()),
                remote_path: packet.file_name().clone(),
                local_path: String::new(),
                peer: peer_label.clone(),
                host: parse_peer_host(&peer_label),
                direction: "incoming".to_string(),
                progress: 0,
                total: packet.file_size(),
                status: 1,
            };
            file_transfers
                .lock()
                .unwrap()
                .insert(transfer.key.clone(), transfer.clone());
            let _ = file_app.emit("incoming-file", transfer);
            emit_snapshot(&file_app);
        };

        let sending_app = data_app.clone();
        let sending_transfers = transfers.clone();
        let sending_callback = move |packet: &FileSendingPacket, _peer: Option<&Peer>| {
            let key = outgoing_key(packet.file_id());
            let mut map = sending_transfers.lock().unwrap();
            let entry = map.entry(key.clone()).or_insert_with(|| Transfer {
                key: key.clone(),
                file_id: packet.file_id(),
                file_name: format!("Send #{}", packet.file_id()),
                remote_path: String::new(),
                local_path: String::new(),
                peer: String::new(),
                host: String::new(),
                direction: "outgoing".to_string(),
                progress: 0,
                total: packet.total(),
                status: 1,
            });
            entry.progress = packet.progress();
            entry.total = packet.total();
            entry.status = packet.status().to_u8();
            if entry.status == 7 {
                entry.progress = entry.total;
            }
            let transfer = entry.clone();
            drop(map);
            let _ = sending_app.emit("transfer-updated", transfer);
            emit_snapshot(&sending_app);
        };

        let part_app = data_app.clone();
        let part_receiving = receiving.clone();
        let part_transfers = transfers.clone();
        let part_callback = move |packet: &FilePartPacket, _peer: Option<&Peer>| -> bool {
            let mut receiving_map = part_receiving.lock().unwrap();
            let Some(file) = receiving_map.get_mut(&packet.file_id()) else {
                return true;
            };

            let write_result =
                OpenOptions::new()
                    .write(true)
                    .open(&file.path)
                    .and_then(|mut handle| {
                        handle.seek(SeekFrom::Start(packet.offset()))?;
                        handle.write_all(packet.data())
                    });
            if write_result.is_err() {
                return true;
            }

            file.progress = file
                .progress
                .max(packet.offset().saturating_add(packet.length()));
            let progress = file.progress;
            let total = file.total;
            let local_path = file.path.to_string_lossy().to_string();
            if progress >= total && total > 0 {
                receiving_map.remove(&packet.file_id());
            }
            drop(receiving_map);

            let key = incoming_key(packet.file_id());
            if let Some(transfer) = part_transfers.lock().unwrap().get_mut(&key) {
                transfer.progress = progress;
                transfer.total = total;
                transfer.local_path = local_path;
                transfer.status = if progress >= total && total > 0 { 7 } else { 4 };
                let _ = part_app.emit("transfer-updated", transfer.clone());
            }
            emit_snapshot(&part_app);
            false
        };

        let context = DataServiceContext::new(
            data_service.config().text_service_listen_addr,
            data_service.config().data_service_listen_port,
            Arc::new(Box::new(text_callback)),
            Arc::new(Box::new(file_callback)),
            Arc::new(Box::new(sending_callback)),
            Arc::new(Box::new(part_callback)),
            data_service.discovery_service(),
        );
        let _ = DataService::run(context, Box::new(move || data_stop.load(Ordering::SeqCst)));
    }));

    let clipboard_app = app.clone();
    let clipboard_stop = stop.clone();
    let clipboard_state = backend.clipboard.clone();
    let clipboard_service = service.discovery_service();
    let clipboard_config = config.clone();
    threads.push(thread::spawn(move || {
        let mut clipboard = match arboard::Clipboard::new() {
            Ok(clipboard) => clipboard,
            Err(_) => return,
        };

        while !clipboard_stop.load(Ordering::SeqCst) {
            if let Ok(text) = clipboard.get_text() {
                let mut should_send = false;
                if let Some(backend) = clipboard_app.try_state::<Backend>() {
                    let settings = backend.settings.lock().unwrap().clone();
                    let mut state = clipboard_state.lock().unwrap();
                    if state.suppressed_text.as_deref() == Some(text.as_str()) {
                        state.suppressed_text = None;
                    } else if state.last_text != text {
                        state.last_text = text.clone();
                        should_send = settings.send_clipboard_enabled
                            && !settings.send_only_on_double_copy
                            && !text.is_empty();
                    }
                }
                if should_send {
                    shared_anydrop_broadcast_text(
                        text,
                        clipboard_service.clone(),
                        &clipboard_config,
                    );
                    emit_snapshot(&clipboard_app);
                }
            }
            thread::sleep(Duration::from_millis(650));
        }
    }));

    *backend.runtime.lock().unwrap() = Some(ServiceRuntime {
        service,
        config,
        stop,
        threads,
    });
    backend.set_status(format!("Online on LAN Group #{}", settings.group_identity));
    Ok(())
}

fn stop_runtime(backend: &Backend) {
    if let Some(runtime) = backend.runtime.lock().unwrap().take() {
        runtime.stop();
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
    // Fire-and-forget: the discovery server listens regardless; this just
    // re-broadcasts our presence to wake up any peers that joined late.
    let cfg = config.clone();
    thread::spawn(move || {
        let _ = DiscoveryService::broadcast_discovery_request(
            cfg.discovery_service_client_port,
            cfg.discovery_service_server_port,
            cfg.group_identifier,
        );
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
    let Some(host) = first_reachable_host(&hosts, config.data_service_listen_port) else {
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
fn send_files_to_peer(
    app: AppHandle,
    backend: State<'_, Backend>,
    hosts: Vec<String>,
    files: Vec<String>,
) -> Result<Snapshot, String> {
    let Some(config) = backend.config() else {
        return Err("Service is offline".to_string());
    };
    let Some(host) = first_reachable_host(&hosts, config.data_service_listen_port) else {
        return Err("No reachable address for peer".to_string());
    };
    let mut queued = 0;
    for file in files {
        if !Path::new(&file).exists() {
            continue;
        }
        // Pre-populate the outgoing transfer so the UI shows real filename
        // before the core's sending callback fires.
        let file_id = backend.next_file_id.fetch_add(1, Ordering::SeqCst);
        let key = outgoing_key(file_id);
        let display_name = file_name(&file);
        let total = fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
        let entry = Transfer {
            key: key.clone(),
            file_id,
            file_name: display_name,
            remote_path: file.clone(),
            local_path: file.clone(),
            peer: host.clone(),
            host: host.clone(),
            direction: "outgoing".to_string(),
            progress: 0,
            total,
            status: 1,
        };
        backend
            .transfers
            .lock()
            .unwrap()
            .insert(key, entry);

        shared_anydrop_try_send_file(host.clone(), file, &config);
        queued += 1;
    }
    backend.set_status(format!("Queued {queued} file(s)"));
    emit_snapshot(&app);
    Ok(backend.snapshot())
}

#[tauri::command]
fn accept_transfer(
    app: AppHandle,
    backend: State<'_, Backend>,
    transfer_key: String,
) -> Result<Snapshot, String> {
    let Some(config) = backend.config() else {
        return Err("Service is offline".to_string());
    };
    let mut transfer = backend
        .transfers
        .lock()
        .unwrap()
        .get(&transfer_key)
        .cloned()
        .ok_or_else(|| "Transfer not found".to_string())?;
    let path = local_receive_path(&transfer.remote_path)?;
    let file = File::create(&path).map_err(|err| err.to_string())?;
    file.set_len(transfer.total)
        .map_err(|err| err.to_string())?;
    backend.receiving.lock().unwrap().insert(
        transfer.file_id,
        ReceivingFile {
            path: path.clone(),
            total: transfer.total,
            progress: 0,
        },
    );

    transfer.local_path = path.to_string_lossy().to_string();
    transfer.status = 3;
    backend
        .transfers
        .lock()
        .unwrap()
        .insert(transfer_key, transfer.clone());
    shared_anydrop_respond_to_file(
        transfer.host.clone(),
        transfer.file_id,
        transfer.total,
        transfer.remote_path.clone(),
        true,
        &config,
    );
    let _ = app.emit("transfer-updated", transfer);
    emit_snapshot(&app);
    Ok(backend.snapshot())
}

#[tauri::command]
fn reject_transfer(
    app: AppHandle,
    backend: State<'_, Backend>,
    transfer_key: String,
) -> Result<Snapshot, String> {
    let Some(config) = backend.config() else {
        return Err("Service is offline".to_string());
    };
    let transfer = backend
        .transfers
        .lock()
        .unwrap()
        .remove(&transfer_key)
        .ok_or_else(|| "Transfer not found".to_string())?;
    shared_anydrop_respond_to_file(
        transfer.host,
        transfer.file_id,
        transfer.total,
        transfer.remote_path,
        false,
        &config,
    );
    emit_snapshot(&app);
    Ok(backend.snapshot())
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
            send_files_to_peer,
            accept_transfer,
            reject_transfer,
            dismiss_transfer,
            open_transfer_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running AnyDrop");
}
