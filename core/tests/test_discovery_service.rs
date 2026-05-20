use anydrop::extension::ip_to_u32::ConvertIpU32;
use anydrop::proto::discovery_packet::DiscoveryPacket;
use anydrop::service::discovery_service::{
    broadcast_addresses_for_local_addresses, DiscoveryService, PeerCollectionType,
};
use protobuf::Message;
use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

fn packet(
    address: Ipv4Addr,
    port: u16,
    need_response: bool,
    group_identifier: u32,
) -> DiscoveryPacket {
    let mut packet = DiscoveryPacket::new();
    packet.set_address(address.to_u32());
    packet.set_server_port(port as u32);
    packet.set_group_identifier(group_identifier);
    packet.set_need_response(need_response);
    packet.set_host_name(String::from("peer"));
    packet
}

fn unused_udp_port() -> u16 {
    UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[test]
fn broadcast_candidates_include_global_and_subnet_broadcasts() {
    let local_addresses =
        HashSet::from([Ipv4Addr::new(192, 168, 8, 23), Ipv4Addr::new(10, 1, 2, 3)]);

    let broadcast_addresses = broadcast_addresses_for_local_addresses(&local_addresses);

    assert!(broadcast_addresses.contains(&Ipv4Addr::BROADCAST));
    assert!(broadcast_addresses.contains(&Ipv4Addr::new(192, 168, 8, 255)));
    assert!(broadcast_addresses.contains(&Ipv4Addr::new(10, 1, 2, 255)));
    assert!(broadcast_addresses.contains(&Ipv4Addr::new(10, 255, 255, 255)));
}

#[test]
fn handle_new_peer_sends_response_as_single_datagram() {
    let server_socket = DiscoveryService::create_broadcast_socket(0).unwrap();
    let response_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    response_socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();

    let response_port = response_socket.local_addr().unwrap().port();
    let peers: PeerCollectionType = Arc::new(Mutex::new(HashSet::new()));
    let local_addresses = HashSet::from([Ipv4Addr::new(192, 0, 2, 10)]);
    let packet = packet(Ipv4Addr::LOCALHOST, response_port, true, 7);

    DiscoveryService::handle_new_peer(
        local_addresses,
        &server_socket,
        peers.clone(),
        None,
        packet,
        7,
    )
    .unwrap();

    let mut buf = [0u8; 4096];
    let (len, _) = response_socket.recv_from(&mut buf).unwrap();
    let response = DiscoveryPacket::parse_from_bytes(&buf[..len]).unwrap();

    assert!(!response.need_response());
    assert_eq!(response.group_identifier(), 7);
    assert_eq!(response.server_port(), response_port as u32);

    let locked = peers.lock().unwrap();
    assert!(locked.iter().any(|peer| peer.host() == "127.0.0.1"));
}

#[test]
fn handle_new_peer_keeps_same_hostname_addresses() {
    let server_socket = DiscoveryService::create_broadcast_socket(0).unwrap();
    let peers: PeerCollectionType = Arc::new(Mutex::new(HashSet::new()));
    let local_addresses = HashSet::from([Ipv4Addr::new(192, 0, 2, 10)]);

    let first = packet(Ipv4Addr::new(192, 168, 1, 30), 9819, false, 7);
    let mut second = packet(Ipv4Addr::new(10, 0, 0, 30), 9819, false, 7);
    second.set_host_name(first.host_name().to_string());

    DiscoveryService::handle_new_peer(
        local_addresses.clone(),
        &server_socket,
        peers.clone(),
        None,
        first,
        7,
    )
    .unwrap();
    DiscoveryService::handle_new_peer(
        local_addresses,
        &server_socket,
        peers.clone(),
        None,
        second,
        7,
    )
    .unwrap();

    let locked = peers.lock().unwrap();
    assert_eq!(locked.len(), 2);
    assert!(locked.iter().any(|peer| peer.host() == "192.168.1.30"));
    assert!(locked.iter().any(|peer| peer.host() == "10.0.0.30"));
}

#[test]
fn discovery_run_accepts_single_datagram_packets() {
    let server_port = unused_udp_port();
    let peers: PeerCollectionType = Arc::new(Mutex::new(HashSet::new()));
    let should_stop = Arc::new(AtomicBool::new(false));
    let run_peers = peers.clone();
    let run_should_stop = should_stop.clone();

    let handle = thread::spawn(move || {
        DiscoveryService::run(
            0,
            server_port,
            run_peers,
            None,
            Box::new(move || run_should_stop.load(Ordering::SeqCst)),
            99,
        )
        .unwrap();
    });

    thread::sleep(Duration::from_millis(250));

    let client = UdpSocket::bind("127.0.0.1:0").unwrap();
    let packet = packet(Ipv4Addr::LOCALHOST, server_port, false, 99);
    let bytes = packet.write_to_bytes().unwrap();
    client
        .send_to(&bytes, SocketAddrV4::new(Ipv4Addr::LOCALHOST, server_port))
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if peers
            .lock()
            .unwrap()
            .iter()
            .any(|peer| peer.host() == "127.0.0.1")
        {
            should_stop.store(true, Ordering::SeqCst);
            handle.join().unwrap();
            return;
        }

        thread::sleep(Duration::from_millis(50));
    }

    should_stop.store(true, Ordering::SeqCst);
    handle.join().unwrap();
    panic!("discovery service did not accept a single datagram discovery packet");
}
