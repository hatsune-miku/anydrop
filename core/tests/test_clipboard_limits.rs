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
            .send_data_progress_with_retry(&data, |_| {})
            .unwrap()
    });
    assert_eq!(
        DataTransmit::from(receiver)
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
            .read_data_progress_with_retry(|_| {})
            .unwrap_err()
            .kind(),
        ErrorKind::UnexpectedEof
    );
}
