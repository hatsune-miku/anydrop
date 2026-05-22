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

    if let Ok(peers_ptr) = service_disc.peers().lock() {
        for peer in peers_ptr.iter() {
            let thread_peer = peer.clone();
            let thread_config = config.clone();
            let thread_text_serialized = text_serialized.clone();
            std::thread::spawn(move || {
                info!(
                    "lib: Sending text to (addr={}:{})",
                    thread_peer.host(),
                    thread_config.data_service_listen_port
                );
                if let Err(e) = DataService::send_once_with_retry(
                    &thread_peer,
                    thread_config.data_service_listen_port,
                    MagicNumbers::Text,
                    &thread_text_serialized,
                    Duration::from_millis(CONNECTION_TIMEOUT_MILLIS),
                ) {
                    error!(
                        "lib: Failed to send text to (addr={}:{}): {}",
                        thread_peer.host(),
                        thread_config.data_service_listen_port,
                        e
                    );
                }
            });
        }
    }
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

    if let Ok(peers_ptr) = service_disc.peers().lock() {
        for peer in peers_ptr.iter() {
            let thread_peer = peer.clone();
            let thread_config = config.clone();
            let thread_serialized = serialized.clone();
            std::thread::spawn(move || {
                info!(
                    "lib: Sending image to (addr={}:{}, {} bytes)",
                    thread_peer.host(),
                    thread_config.data_service_listen_port,
                    thread_serialized.len()
                );
                if let Err(e) = DataService::send_once_with_retry(
                    &thread_peer,
                    thread_config.data_service_listen_port,
                    MagicNumbers::Image,
                    &thread_serialized,
                    Duration::from_millis(CONNECTION_TIMEOUT_MILLIS),
                ) {
                    error!(
                        "lib: Failed to send image to (addr={}:{}): {}",
                        thread_peer.host(),
                        thread_config.data_service_listen_port,
                        e
                    );
                }
            });
        }
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
    if let Ok(logger_config) = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(ConsoleAppender::builder().build())))
        .logger(Logger::builder().build("libanydrop", LevelFilter::Trace))
        .build(Root::builder().appender("stdout").build(LevelFilter::Trace))
    {
        let _ = log4rs::init_config(logger_config);
        info!("lib: Initialized.");
    }
}

pub fn shared_string_from_lengthen_ptr(ptr: *const c_char, len: u32) -> String {
    let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    String::from_utf8_lossy(slice).to_string()
}
