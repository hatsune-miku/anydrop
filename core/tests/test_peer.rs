use anydrop::network::peer::Peer;

/// Equation of two peers is determined by their concrete network endpoint.
#[test]
fn host() {
    let peer1 = Peer::new(
        &String::from("114.51.41.91"),
        9818,
        Some(&String::from("B612")),
    );
    let peer2 = Peer::new(
        &String::from("114.51.41.91"),
        9819,
        Some(&String::from("B612")),
    );
    let peer3 = Peer::new(
        &String::from("111.111.11.1"),
        9819,
        Some(&String::from("Jarilo-VI")),
    );

    assert!(peer1 != peer2);
    assert!(peer1 != peer3);
    assert_eq!(peer2.to_string(), String::from("B612@114.51.41.91:9819"));
}

#[test]
fn same_hostname_keeps_multiple_addresses() {
    let peer1 = Peer::new(
        &String::from("192.168.1.10"),
        9819,
        Some(&String::from("desktop")),
    );
    let peer2 = Peer::new(
        &String::from("10.0.0.10"),
        9819,
        Some(&String::from("desktop")),
    );

    assert!(peer1 != peer2);
}
