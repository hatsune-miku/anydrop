//! Receive-side handler for clipboard image packets.

use crate::packet::data::image_packet::ImagePacket;
use crate::packet::protocol::serialize::Serialize;
use crate::service::handler::context::{ConnectionControl, HandlerContext};
use log::{info, warn};

pub fn handle(context: HandlerContext) -> ConnectionControl {
    let packet = match ImagePacket::deserialize(context.packet().data()) {
        Ok(p) => p,
        Err(e) => {
            warn!("Failed to deserialize image packet ({:?}).", e);
            return ConnectionControl::CloseConnection;
        }
    };
    let peer = context
        .data_service_context()
        .discovery_service()
        .peer_lookup(&context.socket_addr());
    let peer = match peer {
        Some(ref p) => Some(p),
        None => None,
    };
    info!(
        "Received image packet from {} ({} bytes).",
        context.socket_addr(),
        packet.png_bytes().len()
    );
    (context.data_service_context().image_callback())(&packet, peer);
    ConnectionControl::CloseConnection
}
