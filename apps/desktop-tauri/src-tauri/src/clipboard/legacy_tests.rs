use super::*;
use anydrop::packet::{
    data::{image_packet::ImagePacket, text_packet::TextPacket},
    data_packet::DataPacket,
    protocol::serialize::Serialize,
};

// Frozen legacy framing (rc.51 / f044eaf): no message ID, origin, or hop count.
// Keep these independent of the current serializer so protocol drift is caught.
const TEXT: &str = "legacy 文本";
const TEXT_FRAME: &[u8] = &[
    27, 0, 0, 0, 64, 57, 19, 0, 0, 0, 13, 0, 0, 0, 108, 101, 103, 97, 99, 121, 32, 230, 150, 135,
    230, 156, 172, 6, 7, 9, 0,
];
const IMAGE_FRAME: &[u8] = &[
    82, 0, 0, 0, 68, 57, 74, 0, 0, 0, 70, 0, 0, 0, 137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13,
    73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68,
    65, 84, 120, 156, 99, 56, 169, 33, 252, 31, 0, 4, 198, 2, 4, 122, 89, 113, 217, 0, 0, 0, 0, 73,
    69, 78, 68, 174, 66, 96, 130, 37, 0,
];

#[test]
fn legacy_text_and_image_wire_format_is_unchanged() {
    for frame in [TEXT_FRAME, IMAGE_FRAME] {
        let packet = DataPacket::deserialize(&frame[4..].to_vec()).unwrap();
        let payload = match packet.magic_number() {
            0x3940 => {
                let text = TextPacket::deserialize(packet.data()).unwrap();
                assert_eq!(text.text(), TEXT);
                text.serialize()
            }
            0x3944 => {
                let image = ImagePacket::deserialize(packet.data()).unwrap();
                assert_eq!(io::decode(image.png_bytes()).unwrap().dimensions(), (1, 1));
                image.serialize()
            }
            _ => panic!("unexpected legacy packet"),
        };
        let encoded = DataPacket::new(packet.magic_number(), &payload).serialize();
        let mut wire = (encoded.len() as u32).to_le_bytes().to_vec();
        wire.extend_from_slice(&encoded);
        assert_eq!(wire, frame);
    }
}

#[cfg(any(windows, target_os = "macos"))]
#[test]
#[ignore = "Uses the system clipboard; run only on an isolated test desktop"]
fn legacy_duplicate_and_delayed_network_writes_never_rebroadcast() {
    use anydrop::service::{
        context::data_service_context::DataServiceContext, data_service::DataService,
    };
    use std::{
        io::Write,
        net::{SocketAddr, TcpListener, TcpStream},
        sync::mpsc,
    };

    struct Receiver {
        stop: Arc<AtomicBool>,
        worker: Option<std::thread::JoinHandle<()>>,
    }
    impl Drop for Receiver {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            self.worker.take().unwrap().join().unwrap();
        }
    }

    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let state = Arc::new(Mutex::new(ClipboardState::default()));
    let text_state = state.clone();
    let image_state = state.clone();
    let (tx, rx) = mpsc::channel();
    let image_tx = tx.clone();
    let context = DataServiceContext::new(
        "127.0.0.1".into(),
        port,
        Arc::new(Box::new(move |packet, _| {
            tx.send(write_text(&text_state, packet.text())).unwrap();
        })),
        Arc::new(Box::new(move |packet, _| {
            let result =
                io::decode(packet.png_bytes()).and_then(|image| write_image(&image_state, &image));
            image_tx.send(result).unwrap();
        })),
        Arc::new(DiscoveryService::new()),
    );
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let _receiver = Receiver {
        stop,
        worker: Some(std::thread::spawn(move || {
            DataService::run(
                context,
                Box::new(move || worker_stop.load(Ordering::SeqCst)),
            )
            .unwrap();
        })),
    };
    let mut ready = false;
    for _ in 0..100 {
        if TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
            ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(ready, "receiver started");
    let mut clipboard = arboard::Clipboard::new().unwrap();
    clipboard.set_text("initial").unwrap();
    let mut changes = Changes::new(io::revision());

    // Simulate an older peer with multiple addresses, and even one that echoes
    // every delivery. The new receiver must terminate the loop by itself.
    for frame in [TEXT_FRAME, IMAGE_FRAME] {
        for round in 0..3 {
            if round == 2 {
                // Beyond the former 1.5 s network / 2 s suppression windows.
                std::thread::sleep(Duration::from_millis(2100));
            }
            for _ in 0..3 {
                let mut stream =
                    TcpStream::connect_timeout(&address, Duration::from_secs(1)).unwrap();
                stream.write_all(frame).unwrap();
                drop(stream);
                rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
            }
            if round == 1 {
                std::thread::sleep(Duration::from_millis(2100));
            }
            // Multiple writes can coalesce before a poll. Extra clipboard reads
            // and delayed notifications must also leave this write suppressed.
            for _ in 0..5 {
                let content = io::read().unwrap();
                assert_eq!(matches!(content, Content::Text(_)), frame == TEXT_FRAME);
                match &content {
                    Content::Text(text) => assert_eq!(text, TEXT),
                    Content::Image(image) => assert_eq!(image.dimensions(), (1, 1)),
                }
                let state = state.lock().unwrap();
                assert!(
                    !changes.needs_read(io::revision().unwrap(), state.written_revision),
                    "legacy receive became an outgoing clipboard change"
                );
                drop(state);
                std::thread::sleep(Duration::from_millis(20));
            }
            // A deliberate copy of the same content still sends; this must not
            // be implemented by restoring the old content/time suppression.
            match io::read().unwrap() {
                Content::Text(text) => clipboard.set_text(text).unwrap(),
                Content::Image(image) => clipboard
                    .set_image(arboard::ImageData {
                        width: image.width() as usize,
                        height: image.height() as usize,
                        bytes: std::borrow::Cow::Owned(image.into_raw()),
                    })
                    .unwrap(),
            }
            assert!(changes.needs_read(
                io::revision().unwrap(),
                state.lock().unwrap().written_revision
            ));
            assert!(changes.copied(fingerprint(&io::read().unwrap()), Instant::now(), false));
        }
    }
}
