//! Shared helpers reused by the desktop wrapper. File-related helpers were
//! removed when file transfer migrated to the QUIC `transfer` module — only
//! text broadcast, the data-service runner, and init/utility code remain.

use crate::packet::data::image_packet::ImagePacket;
use crate::packet::data::magic_numbers::MagicNumbers;
use crate::packet::data::text_packet::TextPacket;
use crate::packet::protocol::serialize::Serialize;
use crate::service::anydrop_service::AnyDropServiceConfig;
use crate::service::context::data_service_context::DataServiceContext;
use crate::service::data_service::DataService;
use crate::service::discovery_service::DiscoveryService;
use crate::service::ShouldInterruptFunctionType;
use log::{error, info, LevelFilter};
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Logger, Root};
use log4rs::Config;
use std::os::raw::c_char;
use std::sync::Arc;
use std::time::Duration;

pub const CONNECTION_TIMEOUT_MILLIS: u64 = 3000;
pub const ANYDROP_VERSION: i32 = 20230802;
pub const ANYDROP_COMPATIBLE_NUMBER: i32 = 4;

pub fn shared_anydrop_version_code() -> String {
    String::from("\\^O^/")
}

pub fn shared_anydrop_broadcast_text(
    text: String,
    service_disc: Arc<DiscoveryService>,
    config: &AnyDropServiceConfig,
) {
    let packet = match TextPacket::new(text) {
        Ok(packet) => packet,
        Err(err) => {
            error!("lib: Failed to create text packet: {}", err);
            return;
        }
    };
    let text_serialized = Arc::new(packet.serialize());

    broadcast_to_devices(service_disc, config, MagicNumbers::Text, text_serialized);
}

pub fn shared_anydrop_broadcast_image(
    png_bytes: Vec<u8>,
    service_disc: Arc<DiscoveryService>,
    config: &AnyDropServiceConfig,
) {
    let packet = match ImagePacket::new(png_bytes) {
        Ok(p) => p,
        Err(err) => {
            error!("lib: Failed to create image packet: {:?}", err);
            return;
        }
    };
    let serialized = Arc::new(packet.serialize());

    broadcast_to_devices(service_disc, config, MagicNumbers::Image, serialized);
}

fn group_device_addresses(
    peers: Vec<crate::network::peer::Peer>,
) -> Vec<Vec<crate::network::peer::Peer>> {
    let mut devices = std::collections::BTreeMap::<String, Vec<_>>::new();
    for peer in peers {
        let key = if peer.device_id().is_empty() {
            format!("host:{}", peer.host())
        } else {
            format!("device:{}", peer.device_id())
        };
        devices.entry(key).or_default().push(peer);
    }
    devices.into_values().collect()
}

fn broadcast_to_devices(
    service: Arc<DiscoveryService>,
    config: &AnyDropServiceConfig,
    kind: MagicNumbers,
    bytes: Arc<Vec<u8>>,
) {
    let peers = service
        .peers()
        .lock()
        .map(|p| p.iter().cloned().collect())
        .unwrap_or_default();
    for candidates in group_device_addresses(peers) {
        let bytes = bytes.clone();
        let port = config.data_service_listen_port;
        let magic = kind.value();
        std::thread::spawn(move || {
            let mut last_error = String::from("no IPv4 address");
            let ips: Vec<_> = candidates
                .iter()
                .filter_map(|p| p.host().parse().ok())
                .collect();
            // Fresh ranking on every copy handles Wi-Fi/Ethernet changes. Send
            // once per device; try its other addresses only on failure.
            for ip in crate::util::network::rank_peer_ips(&ips) {
                let peer = candidates
                    .iter()
                    .find(|p| p.host() == &ip.to_string())
                    .unwrap();
                match DataService::send_once_with_retry(
                    peer,
                    port,
                    MagicNumbers::from(magic).unwrap(),
                    &bytes,
                    Duration::from_millis(CONNECTION_TIMEOUT_MILLIS),
                ) {
                    Ok(()) => return,
                    Err(error) => last_error = format!("{}:{port}: {error}", peer.host()),
                }
            }
            error!("lib: clipboard delivery failed for all addresses of a device ({last_error})");
        });
    }
}

#[cfg(test)]
mod clipboard_delivery_tests {
    use super::*;
    use crate::network::peer::Peer;
    #[test]
    fn groups_interfaces_by_device_without_merging_equal_hostnames() {
        let peers = vec![
            Peer::new(&"192.168.1.2".into(), 9819, Some(&"PC".into())).with_device_id("a"),
            Peer::new(&"10.0.0.2".into(), 9819, Some(&"PC".into())).with_device_id("a"),
            Peer::new(&"192.168.1.3".into(), 9819, Some(&"PC".into())).with_device_id("b"),
            Peer::new(&"192.168.1.4".into(), 9819, Some(&"PC".into())),
        ];
        let devices = group_device_addresses(peers);
        assert_eq!(devices.len(), 3);
        assert_eq!(devices.iter().map(Vec::len).sum::<usize>(), 4);
        assert_eq!(devices.iter().filter(|d| d.len() == 2).count(), 1);
    }
}

pub fn shared_anydrop_data_service(
    context: DataServiceContext,
    config: &AnyDropServiceConfig,
    should_interrupt: ShouldInterruptFunctionType,
) {
    info!(
        "lib: Data service starting (addr={},port={})",
        config.text_service_listen_addr, config.data_service_listen_port
    );

    let _ = DataService::run(context, should_interrupt);

    info!("lib: Data service stopped");
}

pub fn shared_anydrop_init() {
    // Logging policy:
    //   * NEVER default to Trace/Debug — those drown the console in per-packet
    //     (`mdns_sd`) and per-poll (`polling`) spam and churn strings hot.
    //   * Debug builds: Info. Release builds: Error (production stays quiet).
    //   * `RUST_LOG=<level>` overrides the root level for ad-hoc debugging.
    //   * The chronically noisy dependencies are pinned to Warn regardless, so
    //     even bumping the root level can't unleash their flood.
    //   * Console (stdout) only — we never write logs to disk.
    let default_root = if cfg!(debug_assertions) {
        LevelFilter::Info
    } else {
        LevelFilter::Error
    };
    let root_level = std::env::var("RUST_LOG")
        .ok()
        .and_then(|v| v.trim().parse::<LevelFilter>().ok())
        .unwrap_or(default_root);

    if let Ok(logger_config) = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(ConsoleAppender::builder().build())))
        .logger(Logger::builder().build("mdns_sd", LevelFilter::Warn))
        .logger(Logger::builder().build("polling", LevelFilter::Warn))
        .logger(Logger::builder().build("mio", LevelFilter::Warn))
        .logger(Logger::builder().build("quinn", LevelFilter::Warn))
        .logger(Logger::builder().build("quinn_proto", LevelFilter::Warn))
        .build(Root::builder().appender("stdout").build(root_level))
    {
        let _ = log4rs::init_config(logger_config);
    }
}

pub fn shared_string_from_lengthen_ptr(ptr: *const c_char, len: u32) -> String {
    let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    String::from_utf8_lossy(slice).to_string()
}
