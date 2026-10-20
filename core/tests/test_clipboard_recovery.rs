use anydrop::{
    lib_util::{shared_anydrop_broadcast_image, shared_anydrop_broadcast_text},
    network::peer::Peer,
    service::{
        anydrop_service::AnyDropServiceConfig, context::data_service_context::DataServiceContext,
        data_service::DataService, discovery_service::DiscoveryService,
    },
};
use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};

#[test]
fn text_and_images_keep_working_across_repeated_receiver_restarts() {
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let discovery = Arc::new(DiscoveryService::new());
    for address in ["127.0.0.1", "127.0.0.2"] {
        discovery.peers().lock().unwrap().insert(
            Peer::new(&address.into(), port, None).with_device_id("one-device-two-interfaces"),
        );
    }
    let config = AnyDropServiceConfig {
        data_service_listen_port: port,
        text_service_listen_addr: "0.0.0.0".into(),
        discovery_service_client_port: 0,
        discovery_service_server_port: 0,
        group_identifier: 7,
    };
    let (tx, rx) = mpsc::channel();
    let image_tx = tx.clone();
    let context = DataServiceContext::new(
        "0.0.0.0".into(),
        port,
        Arc::new(Box::new(move |packet, _| {
            tx.send(("text", packet.text().as_bytes().to_vec()))
                .unwrap();
        })),
        Arc::new(Box::new(move |packet, _| {
            image_tx
                .send(("image", packet.png_bytes().to_vec()))
                .unwrap();
        })),
        discovery.clone(),
    );
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(8, 8, image::Rgba([1, 2, 3, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let png = png.into_inner();
    for _ in 0..12 {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker_context = context.clone();
        let worker = std::thread::spawn(move || {
            DataService::run(
                worker_context,
                Box::new(move || worker_stop.load(Ordering::SeqCst)),
            )
            .unwrap()
        });
        let mut ready = None;
        for _ in 0..100 {
            if let Ok(stream) = TcpStream::connect_timeout(
                &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
                Duration::from_millis(100),
            ) {
                ready = Some(stream);
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // An interrupted clipboard transfer must not stop the accept loop.
        let mut broken = ready.expect("receiver started");
        broken.write_all(&100u32.to_le_bytes()).unwrap();
        broken.write_all(b"partial").unwrap();
        drop(broken);
        for _ in 0..2 {
            shared_anydrop_broadcast_text("same text".into(), discovery.clone(), &config);
            assert_eq!(
                rx.recv_timeout(Duration::from_secs(5)).unwrap(),
                ("text", b"same text".to_vec())
            );
        }
        shared_anydrop_broadcast_image(png.clone(), discovery.clone(), &config);
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            ("image", png.clone())
        );
        assert!(
            rx.recv_timeout(Duration::from_millis(40)).is_err(),
            "one delivery per device, not per address"
        );
        stop.store(true, Ordering::SeqCst);
        worker.join().unwrap();
    }
}
