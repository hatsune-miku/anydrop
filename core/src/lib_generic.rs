//! C ABI exposed to mobile clients.
//!
//! File-transfer entry points were removed when v1 was retired in favor of the
//! QUIC `transfer` module. Mobile clients will need to be updated to either
//! use the new QUIC stack or drop file transfer entirely. What remains:
//! discovery service, text broadcast, init, and version helpers.

extern crate core;

use crate::lib_util::{
    shared_anydrop_broadcast_text, shared_anydrop_data_service, shared_anydrop_init,
    shared_anydrop_version_code, shared_string_from_lengthen_ptr, ANYDROP_COMPATIBLE_NUMBER,
    ANYDROP_VERSION,
};
use crate::network::peer::Peer;
use crate::packet::data::image_packet::ImagePacket;
use crate::packet::data::text_packet::TextPacket;
use crate::service;
use crate::service::anydrop_service::AnyDropService;
use crate::service::context::data_service_context::DataServiceContext;
use crate::service::discovery_service::DiscoveryService;
use log::{error, info};
use std::os::raw::c_char;
use std::ptr::copy;
use std::sync::Arc;

#[export_name = "anydrop_version"]
pub extern "C" fn anydrop_version() -> i32 {
    ANYDROP_VERSION
}

#[export_name = "anydrop_compatibility_number"]
pub extern "C" fn anydrop_compatibility_number() -> i32 {
    ANYDROP_COMPATIBLE_NUMBER
}

#[export_name = "anydrop_version_string"]
pub extern "C" fn anydrop_version_string(buffer: *mut c_char) -> u64 {
    let version = shared_anydrop_version_code();
    let version = version.as_bytes();
    let len = version.len();
    unsafe {
        copy(version.as_ptr(), buffer as *mut u8, len);
        buffer.offset(len as isize).write(0);
    }
    len as u64
}

#[export_name = "anydrop_init"]
pub extern "C" fn anydrop_init() {
    shared_anydrop_init()
}

#[export_name = "anydrop_create"]
pub unsafe extern "C" fn anydrop_create_service(
    discovery_service_server_port: u16,
    discovery_service_client_port: u16,
    text_service_listen_addr: *mut c_char,
    text_service_listen_addr_len: u32,
    text_service_listen_port: u16,
    group_identifier: u32,
) -> *mut AnyDropService {
    let addr =
        shared_string_from_lengthen_ptr(text_service_listen_addr, text_service_listen_addr_len);

    let config = service::anydrop_service::AnyDropServiceConfig {
        discovery_service_server_port,
        discovery_service_client_port,
        text_service_listen_addr: addr.clone(),
        data_service_listen_port: text_service_listen_port,
        group_identifier,
    };
    let anydrop = AnyDropService::new(&config);
    let anydrop = match anydrop {
        Ok(anydrop) => Box::into_raw(Box::new(anydrop)),
        Err(_) => std::ptr::null_mut(),
    };

    info!(
        "lib: AnyDrop config created (addr={}:{},gid={})",
        addr, text_service_listen_port, group_identifier
    );

    anydrop
}

#[export_name = "anydrop_lan_discovery_service"]
pub extern "C" fn anydrop_lan_discovery_service(
    anydrop_ptr: *mut AnyDropService,
    should_interrupt: extern "C" fn() -> bool,
) {
    let anydrop = unsafe { &mut *anydrop_ptr };
    let config = anydrop.config();

    let service_disc = anydrop.discovery_service();
    let peers_ptr = service_disc.peers();
    drop(service_disc);

    info!(
        "lib: Discovery service starting (cp={},sp={},gid={})",
        config.discovery_service_client_port,
        config.discovery_service_server_port,
        config.group_identifier
    );

    let _ = DiscoveryService::run(
        config.discovery_service_client_port,
        config.discovery_service_server_port,
        peers_ptr,
        None,
        Box::new(move || should_interrupt()),
        config.group_identifier,
        crate::util::os::OSUtil::hostname(),
    );

    info!("lib: Discovery service stopped.");
}

#[export_name = "anydrop_data_service"]
pub extern "C" fn anydrop_data_service(
    anydrop_ptr: *mut AnyDropService,
    text_callback_c: extern "C" fn(
        *const c_char, /* text */
        u32,           /* text_len */
        *const c_char, /* socket_addr */
        u32,           /* socket_addr_len */
    ),
    should_interrupt: extern "C" fn() -> bool,
) {
    let anydrop = unsafe { &mut *anydrop_ptr };
    let config = anydrop.config();

    let should_interrupt_callback = move || should_interrupt();

    let text_callback = move |text_packet: &TextPacket, peer: Option<&Peer>| {
        let text_cstr = text_packet.text().as_ptr();
        let socket_addr_str = match peer {
            Some(p) => p.to_string(),
            None => Peer::default().to_string(),
        };
        let socket_addr_cstr = socket_addr_str.as_ptr();
        text_callback_c(
            text_cstr as *const c_char,
            text_packet.text().len() as u32,
            socket_addr_cstr as *const c_char,
            socket_addr_str.len() as u32,
        );
    };

    // Mobile clients don't have a clipboard image story yet — provide a
    // no-op image callback so the wire format works but received images are
    // silently dropped.
    let image_callback = move |_pkt: &ImagePacket, _peer: Option<&Peer>| {};
    let context = DataServiceContext::new(
        config.text_service_listen_addr.to_string(),
        config.data_service_listen_port,
        Arc::new(Box::new(text_callback)),
        Arc::new(Box::new(image_callback)),
        anydrop.discovery_service().clone(),
    );

    shared_anydrop_data_service(context, &config, Box::new(should_interrupt_callback));
}

#[export_name = "anydrop_get_peers"]
pub extern "C" fn anydrop_get_peers(anydrop_ptr: *mut AnyDropService, buffer: *mut c_char) -> u32 {
    let anydrop = unsafe { &mut *anydrop_ptr };
    let service_disc = anydrop.discovery_service();

    if let Ok(peers_ptr) = service_disc.peers().lock() {
        let joined = peers_ptr
            .iter()
            .map(|peer| peer.to_string())
            .collect::<Vec<String>>()
            .join(",");
        let bytes = joined.as_bytes();

        info!("lib: Get peers (peers={})", joined);

        unsafe {
            copy(bytes.as_ptr(), buffer as *mut u8, bytes.len());
            *buffer.offset(bytes.len() as isize) = 0;
        }
        return bytes.len() as u32;
    }
    error!("lib: Failed to get peers");
    0
}

#[export_name = "anydrop_broadcast_text"]
pub extern "C" fn anydrop_broadcast_text(
    anydrop_ptr: *mut AnyDropService,
    text: *mut c_char,
    len: u32,
) {
    if text.is_null() || len < 1 {
        return;
    }

    let anydrop = unsafe { &mut *anydrop_ptr };
    let config = anydrop.config();
    let service_disc = anydrop.discovery_service();
    let text = shared_string_from_lengthen_ptr(text, len);

    shared_anydrop_broadcast_text(text, service_disc, &config);
}
