use anydrop::packet::{
    data::{image_packet::ImagePacket, text_packet::TextPacket},
    data_packet::DataPacket,
    data_transmission::DataTransmit,
    protocol::serialize::Serialize,
};
use anydrop::{MAX_CLIPBOARD_BYTES, MAX_CLIPBOARD_FRAME_BYTES};
use std::{
    io::{ErrorKind, Write},
    net::{TcpListener, TcpStream},
    time::Duration,
};

#[test]
fn text_limit_is_utf8_bytes_and_malformed_lengths_never_panic() {
    let text = "😀".repeat(MAX_CLIPBOARD_BYTES / 4);
    let packet = TextPacket::new(text.clone()).unwrap();
    assert_eq!(
        TextPacket::deserialize(&packet.serialize()).unwrap().text,
        text
    );
    assert!(TextPacket::new(format!("{text}a")).is_err());
    for length in [0u32, 1, 20, MAX_CLIPBOARD_BYTES as u32 + 1, u32::MAX] {
        let mut bytes = length.to_le_bytes().to_vec();
        bytes.extend([0, 0]);
        assert!(TextPacket::deserialize(&bytes).is_err());
    }
    let mut trailing = TextPacket::new("x".into()).unwrap().serialize();
    trailing.push(0);
    assert!(TextPacket::deserialize(&trailing).is_err());
}
#[test]
fn image_uses_same_payload_limit() {
    let mut png = vec![0; MAX_CLIPBOARD_BYTES];
    png[..8].copy_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
    let packet = ImagePacket::new(png.clone()).unwrap();
    assert_eq!(
        ImagePacket::deserialize(&packet.serialize())
            .unwrap()
            .png_bytes(),
        png
    );
    png.push(0);
    assert!(ImagePacket::new(png).is_err());
}
fn pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let sender = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let receiver = listener.accept().unwrap().0;
    receiver
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    (sender, receiver)
}
#[test]
fn full_limit_frame_roundtrips_including_protocol_overhead() {
    let data = DataPacket::new(
        0x3940,
        &TextPacket::new("a".repeat(MAX_CLIPBOARD_BYTES))
            .unwrap()
            .serialize(),
    )
    .serialize();
    assert_eq!(data.len(), MAX_CLIPBOARD_FRAME_BYTES);
    let (sender, receiver) = pair();
    let expected = data.clone();
    let task = std::thread::spawn(move || {
        DataTransmit::from(sender)
            .unwrap()
            .send_data_progress_with_retry(&data, |_| {})
            .unwrap()
    });
    assert_eq!(
        DataTransmit::from(receiver)
            .unwrap()
            .read_data_progress_with_retry(|_| {})
            .unwrap(),
        expected
    );
    task.join().unwrap();
}
#[test]
fn oversized_frame_rejected_before_allocation_and_eof_terminates() {
    let (mut sender, receiver) = pair();
    sender.write_all(&u32::MAX.to_le_bytes()).unwrap();
    assert_eq!(
        DataTransmit::from(receiver)
            .unwrap()
            .read_data_progress_with_retry(|_| {})
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
    let (mut sender, receiver) = pair();
    sender.write_all(&20u32.to_le_bytes()).unwrap();
    sender.write_all(&[1, 2]).unwrap();
    drop(sender);
    assert_eq!(
        DataTransmit::from(receiver)
            .unwrap()
            .read_data_progress_with_retry(|_| {})
            .unwrap_err()
            .kind(),
        ErrorKind::UnexpectedEof
    );
}

#[test]
fn nonblocking_connection_waits_for_fragmented_image_data() {
    // Windows accept() inherits the listener's nonblocking mode. A small
    // loopback packet can arrive before read(), hiding this on the old tests.
    let (mut sender, receiver) = pair();
    receiver.set_nonblocking(true).unwrap();
    let mut image = image::RgbaImage::new(256, 256);
    let mut seed = 17u32;
    for pixel in image.pixels_mut() {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        *pixel = image::Rgba([seed as u8, (seed >> 8) as u8, (seed >> 16) as u8, 255]);
    }
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let packet = DataPacket::new(
        0x3944,
        &ImagePacket::new(png.into_inner()).unwrap().serialize(),
    )
    .serialize();
    let expected = packet.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let task = std::thread::spawn(move || {
        let mut receiver = DataTransmit::from(receiver).unwrap();
        ready_tx.send(()).unwrap();
        receiver.read_data_progress_with_retry(|_| {})
    });
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    let mut wire = (packet.len() as u32).to_le_bytes().to_vec();
    wire.extend(packet);
    // Split even the length header, then send an image over many TCP packets.
    let mut sent = sender.write_all(&wire[..2]);
    std::thread::sleep(Duration::from_millis(50));
    for chunk in wire[2..].chunks(4096) {
        if sent.is_err() {
            break;
        }
        sent = sender.write_all(chunk);
        std::thread::sleep(Duration::from_millis(2));
    }
    let received = task.join().unwrap();
    assert_eq!(received.unwrap(), expected);
    sent.unwrap();
}
