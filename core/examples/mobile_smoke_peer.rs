//! Isolated LAN interoperability probe. Does not read or write the host clipboard.
//! Run with `cargo run -p anydrop --example mobile_smoke_peer -- 4294967200`.
use anydrop::packet::data::{magic_numbers::MagicNumbers, text_packet::TextPacket};
use anydrop::packet::data_packet::DataPacket;
use anydrop::packet::protocol::serialize::Serialize;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

fn send(address: SocketAddr, text: &str) -> std::io::Result<()> {
    let inner = TextPacket::new(text.to_owned()).unwrap().serialize();
    let packet = DataPacket::new(MagicNumbers::Text.value(), &inner).serialize();
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(3))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    stream.write_all(&(packet.len() as u32).to_le_bytes())?;
    stream.write_all(&packet)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let group: u32 = std::env::args()
        .nth(1)
        .ok_or("Supply an isolated test frequency")?
        .parse()?;
    let listener = TcpListener::bind("0.0.0.0:0")?;
    listener.set_nonblocking(true)?;
    println!("Probe TCP port: {}", listener.local_addr()?.port());
    let daemon = ServiceDaemon::new()?;
    let service_type = "_anydrop._udp.local.";
    let mut properties = HashMap::new();
    properties.insert("g".to_string(), group.to_string());
    properties.insert("dn".to_string(), "Rust interoperability probe".to_string());
    properties.insert("did".to_string(), "anydrop-mobile-smoke-probe".to_string());
    properties.insert("cap".to_string(), "clipboardText".to_string());
    let service = ServiceInfo::new(
        service_type,
        "AnyDrop-mobile-smoke",
        "anydrop-mobile-smoke.local.",
        "",
        listener.local_addr()?.port(),
        Some(properties),
    )?
    .enable_addr_auto();
    daemon.register(service)?;
    let events = daemon.browse(service_type)?;
    let mut targets = HashMap::new();
    let began = Instant::now();
    println!("Test frequency {group}; only synthetic text; exits after 15 minutes.");
    while began.elapsed() < Duration::from_secs(900) {
        while let Ok(event) = events.try_recv() {
            if let ServiceEvent::ServiceResolved(service) = event {
                if service
                    .get_property_val_str("g")
                    .and_then(|g| g.parse::<u32>().ok())
                    != Some(group)
                    || service.get_property_val_str("did") == Some("anydrop-mobile-smoke-probe")
                {
                    continue;
                }
                if let Some(ip) = service
                    .get_addresses()
                    .iter()
                    .find(|ip| ip.is_ipv4() && !ip.is_loopback())
                {
                    let address = SocketAddr::new(*ip, service.get_port());
                    if !targets.contains_key(&address) {
                        println!("Discovered mobile peer at {address}");
                        targets.insert(address, (Instant::now(), false));
                    }
                }
            }
        }
        for (address, (seen, sent)) in targets.iter_mut() {
            if !*sent && seen.elapsed() > Duration::from_secs(4) {
                let result = send(*address, "AnyDrop 联通测试：Rust → 手机 😀");
                println!("Rust → mobile: {result:?}");
                *sent = true;
            }
        }
        if let Ok((mut stream, address)) = listener.accept() {
            stream.set_read_timeout(Some(Duration::from_secs(3)))?;
            let mut header = [0u8; 4];
            stream.read_exact(&mut header)?;
            let size = u32::from_le_bytes(header) as usize;
            if !(14..=anydrop::MAX_CLIPBOARD_FRAME_BYTES).contains(&size) {
                continue;
            }
            let mut bytes = vec![0; size];
            stream.read_exact(&mut bytes)?;
            let packet = DataPacket::deserialize(&bytes).map_err(|e| format!("{e:?}"))?;
            if packet.magic_number() != MagicNumbers::Text.value() || packet.data().len() < 6 {
                continue;
            }
            let payload = packet.data();
            if u32::from_le_bytes(payload[..4].try_into()?) as usize + 6 != payload.len() {
                continue;
            }
            let text = TextPacket::deserialize(payload)?;
            println!("mobile → Rust ({address}): {:?}", text.text);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    daemon.shutdown()?;
    Ok(())
}
