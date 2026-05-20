use crate::extension::ip_to_u32::ConvertIpU32;
use crate::network::peer::Peer;
use crate::proto::discovery_packet::DiscoveryPacket;
use crate::service::ShouldInterruptFunctionType;
use crate::util::os::OSUtil;
use log::{error, info};
use protobuf::Message;
use std::collections::{HashMap, HashSet};
use std::io;
use std::io::ErrorKind::{TimedOut, WouldBlock};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const DISCOVERY_TIMEOUT_MILLIS: u64 = 1000;
const MAX_DISCOVERY_PACKET_BYTES: usize = 4096;

pub type PeerCollectionType = Arc<Mutex<HashSet<Peer>>>;
pub type PeerLastSeenType = Arc<Mutex<HashMap<String, Instant>>>;

trait ToIpV4Addr {
    fn to_ipv4_addr(&self) -> Option<Ipv4Addr>;
}

impl ToIpV4Addr for IpAddr {
    fn to_ipv4_addr(&self) -> Option<Ipv4Addr> {
        match self {
            IpAddr::V4(ip) => Some(*ip),
            IpAddr::V6(_) => None,
        }
    }
}

fn scan_local_addresses() -> Result<HashSet<Ipv4Addr>, local_ip_address::Error> {
    Ok(local_ip_address::list_afinet_netifas()?
        .iter()
        .map(|(_, i)| i)
        .filter(|i| i.is_ipv4() && !i.is_loopback())
        .map(|i| i.to_ipv4_addr().unwrap())
        .collect::<HashSet<Ipv4Addr>>())
}

fn scan_broadcast_addresses() -> Result<HashSet<Ipv4Addr>, local_ip_address::Error> {
    Ok(broadcast_addresses_for_local_addresses(
        &scan_local_addresses()?,
    ))
}

pub fn broadcast_addresses_for_local_addresses(
    local_addresses: &HashSet<Ipv4Addr>,
) -> HashSet<Ipv4Addr> {
    let mut broadcast_addresses = HashSet::new();
    broadcast_addresses.insert(Ipv4Addr::BROADCAST);

    for local_addr in local_addresses {
        if local_addr.is_loopback() || local_addr.is_unspecified() {
            continue;
        }

        let octets = local_addr.octets();
        broadcast_addresses.insert(Ipv4Addr::new(octets[0], octets[1], octets[2], 255));

        if octets[0] == 10 {
            broadcast_addresses.insert(Ipv4Addr::new(10, 255, 255, 255));
        } else if octets[0] == 172 && (16..=31).contains(&octets[1]) {
            broadcast_addresses.insert(Ipv4Addr::new(172, octets[1], 255, 255));
        } else if octets[0] == 192 && octets[1] == 168 {
            broadcast_addresses.insert(Ipv4Addr::new(192, 168, octets[2], 255));
        } else if octets[0] == 169 && octets[1] == 254 {
            broadcast_addresses.insert(Ipv4Addr::new(169, 254, 255, 255));
        }
    }

    broadcast_addresses
}

pub struct DiscoveryService {
    peer_set_ptr: PeerCollectionType,
}

impl DiscoveryService {
    pub fn new() -> Self {
        Self {
            peer_set_ptr: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn create_broadcast_socket(port: u16) -> Result<UdpSocket, io::Error> {
        match UdpSocket::bind(format!("0.0.0.0:{}", port)) {
            Ok(s) => {
                s.set_read_timeout(Some(Duration::from_millis(DISCOVERY_TIMEOUT_MILLIS)))?;
                s.set_broadcast(true)?;
                Ok(s)
            }
            Err(e) => {
                error!("Failed to bind UDP socket: {}", e);
                Err(e)
            }
        }
    }

    pub fn peers(&self) -> PeerCollectionType {
        self.peer_set_ptr.clone()
    }

    fn send_discovery_packet(
        socket: &UdpSocket,
        packet: &DiscoveryPacket,
        target: SocketAddrV4,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let serialized = packet.write_to_bytes()?;
        if serialized.len() > MAX_DISCOVERY_PACKET_BYTES {
            return Err("Discovery packet is too large".into());
        }

        socket.send_to(serialized.as_slice(), target)?;
        Ok(())
    }

    pub fn peer_lookup(&self, socker_address: &SocketAddr) -> Option<Peer> {
        if let Ok(locked) = self.peer_set_ptr.lock() {
            for peer in locked.iter() {
                if *peer.host() == socker_address.ip().to_string() {
                    return Some(peer.clone());
                }
            }
        }
        None
    }

    // Suppress: `std::` can't be omitted but IDEA thinks it can.
    #[allow(unused_qualifications)]
    pub fn handle_new_peer(
        local_addresses: HashSet<Ipv4Addr>,
        server_socket: &UdpSocket,
        peers: PeerCollectionType,
        last_seen: Option<PeerLastSeenType>,
        packet: DiscoveryPacket,
        group_identifier: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let sender_address = packet.address();
        let sender_address_ipv4 = Ipv4Addr::from(sender_address);
        if local_addresses.contains(&sender_address_ipv4) {
            return Err("Received packet from self".into());
        }

        if packet.group_identifier() != group_identifier {
            // Group identity mismatch.
            info!(
                "Dropped packet from different group (mine={}, theirs={}).",
                group_identifier,
                packet.group_identifier()
            );
            return Err("Group identity mismatch".into());
        }

        info!(
            "Received discovery packet from {} - {}",
            packet.host_name(),
            sender_address
        );

        if packet.need_response() {
            // Respond to our new friend on behalf of each local address.
            info!("Responding to discovery request from {}", sender_address);
            let self_hostname = OSUtil::hostname();
            for local_addr_ipv4 in local_addresses {
                let mut response_packet = DiscoveryPacket::new();
                response_packet.set_address(local_addr_ipv4.into());
                response_packet.set_server_port(packet.server_port());
                response_packet.set_group_identifier(group_identifier);
                response_packet.set_need_response(false);
                response_packet.set_host_name(self_hostname.clone());

                match Self::send_discovery_packet(
                    server_socket,
                    &response_packet,
                    SocketAddrV4::new(sender_address_ipv4, packet.server_port() as u16),
                ) {
                    Ok(_) => {
                        info!("Successfully sent response packet to {}", sender_address);
                    }
                    Err(e) => {
                        error!("Failed to send response packet: {}", e);
                        return Err(e);
                    }
                }
            }
        }

        info!("Adding peer {} to peer set.", sender_address);
        if let Ok(mut locked) = peers.lock() {
            locked.insert(Peer::from(
                &sender_address_ipv4,
                packet.server_port() as u16,
                Some(&packet.host_name().to_string()),
            ));
            info!("Added peer {} to peer set.", sender_address);
        }

        if let Some(last_seen_arc) = last_seen {
            if let Ok(mut map) = last_seen_arc.lock() {
                map.insert(sender_address_ipv4.to_string(), Instant::now());
            }
        }

        Ok(())
    }

    pub fn broadcast_discovery_request(
        client_port: u16,
        server_port: u16,
        group_identifier: u32,
    ) -> Result<(), io::Error> {
        let client_socket = Self::create_broadcast_socket(client_port)?;
        let broadcast_addresses = match scan_broadcast_addresses() {
            Ok(x) => x,
            Err(e) => {
                error!("Failed to get broadcast addresses: {}", e);
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to get broadcast addresses: {}", e),
                ));
            }
        };
        let local_addresses = match scan_local_addresses() {
            Ok(x) => x,
            Err(e) => {
                error!("Failed to get local addresses: {}", e);
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to get local addresses: {}", e),
                ));
            }
        };

        let self_hostname = OSUtil::hostname();
        let mut broadcast_packet = DiscoveryPacket::new();
        broadcast_packet.set_server_port(server_port as u32);
        broadcast_packet.set_group_identifier(group_identifier);
        broadcast_packet.set_need_response(true);
        broadcast_packet.set_host_name(self_hostname.clone());

        for broadcast_addr_ipv4 in &broadcast_addresses {
            for local_addr_ipv4 in &local_addresses {
                broadcast_packet.set_address(local_addr_ipv4.clone().to_u32());

                let result = Self::send_discovery_packet(
                    &client_socket,
                    &broadcast_packet,
                    SocketAddrV4::new(*broadcast_addr_ipv4, server_port),
                );

                if result.is_ok() {
                    info!(
                        "Successfully broadcast discovery packet to {}",
                        broadcast_addr_ipv4
                    );
                } else {
                    error!(
                        "Failed to broadcast discovery packet to {}",
                        broadcast_addr_ipv4
                    );
                }
            }
        }
        Ok(())
    }

    pub fn run(
        client_port: u16,
        server_port: u16,
        peer_set_ptr: PeerCollectionType,
        last_seen: Option<PeerLastSeenType>,
        should_interrupt: ShouldInterruptFunctionType,
        group_identifier: u32,
    ) -> Result<(), io::Error> {
        let server_socket = Self::create_broadcast_socket(server_port)?;
        let mut buf = [0u8; MAX_DISCOVERY_PACKET_BYTES];

        // Broadcast discovery request twice to ensure that we are discovered.
        for _ in 0..2 {
            let _ = Self::broadcast_discovery_request(client_port, server_port, group_identifier);
        }

        info!("Discovery service online and ready for connections.");

        loop {
            let packet_size = match server_socket.recv_from(&mut buf) {
                Ok((size, _)) => size,
                Err(e) if e.kind() == WouldBlock || e.kind() == TimedOut => {
                    if should_interrupt() {
                        info!("Discovery service interrupted by caller.");
                        break;
                    }
                    continue;
                }
                Err(e) => {
                    error!("Failed to receive packet size ({})", e);

                    // Broadcast another one to ensure that we are discovered.
                    let _ = Self::broadcast_discovery_request(
                        client_port,
                        server_port,
                        group_identifier,
                    );
                    continue;
                }
            };

            if let Ok(local_addresses) = scan_local_addresses() {
                let packet = match DiscoveryPacket::parse_from_bytes(&buf[..packet_size]) {
                    Ok(x) => x,
                    Err(e) => {
                        error!("Failed to parse discovery packet ({})", e);
                        continue;
                    }
                };
                let _ = Self::handle_new_peer(
                    local_addresses,
                    &server_socket,
                    peer_set_ptr.clone(),
                    last_seen.clone(),
                    packet,
                    group_identifier,
                );
            } else {
                error!("Failed to scan local addresses.");
                if should_interrupt() {
                    info!("Discovery service interrupted by caller.");
                    break;
                }
            }
        }

        Ok(())
    }
}
