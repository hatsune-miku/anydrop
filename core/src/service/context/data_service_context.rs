use crate::packet::data::image_packet::ImagePacket;
use crate::packet::data::text_packet::TextPacket;
use crate::service::data_service::OnPacketReceivedFunctionType;
use crate::service::discovery_service::DiscoveryService;
use std::sync::Arc;

/// Context handed to the legacy TCP data service. Since file transfer moved
/// to the QUIC-based `transfer` module, this carries only clipboard payloads
/// (text + image).
pub struct DataServiceContext {
    host: String,
    port: u16,
    text_callback: OnPacketReceivedFunctionType<TextPacket, ()>,
    image_callback: OnPacketReceivedFunctionType<ImagePacket, ()>,
    discovery_service: Arc<DiscoveryService>,
}

impl DataServiceContext {
    pub fn new(
        host: String,
        port: u16,
        text_callback: OnPacketReceivedFunctionType<TextPacket, ()>,
        image_callback: OnPacketReceivedFunctionType<ImagePacket, ()>,
        discovery_service: Arc<DiscoveryService>,
    ) -> Self {
        Self {
            host,
            port,
            text_callback,
            image_callback,
            discovery_service,
        }
    }

    pub fn host(&self) -> &String {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn text_callback(&self) -> OnPacketReceivedFunctionType<TextPacket, ()> {
        self.text_callback.clone()
    }

    pub fn image_callback(&self) -> OnPacketReceivedFunctionType<ImagePacket, ()> {
        self.image_callback.clone()
    }

    pub fn discovery_service(&self) -> Arc<DiscoveryService> {
        self.discovery_service.clone()
    }
}

impl Clone for DataServiceContext {
    fn clone(&self) -> Self {
        Self {
            host: self.host.clone(),
            port: self.port,
            text_callback: self.text_callback.clone(),
            image_callback: self.image_callback.clone(),
            discovery_service: self.discovery_service.clone(),
        }
    }
}
