use anydrop::packet::data::file_receive_response_packet::FileReceiveResponsePacket;
use anydrop::packet::protocol::serialize::Serialize;

#[test]
fn test_test_file_receive_response_packet() {
    let packet = FileReceiveResponsePacket::new(
        11,
        1024,
        String::from("test中文测试 \\^O^/ 😃 RTL test سلام عليكم 🇯🇵こんにちは؟ *&%^.txt"),
        true,
    );
    let bytes = packet.serialize();
    let packet2 = FileReceiveResponsePacket::deserialize(&bytes).unwrap();
    assert!(packet.eq(&packet2));
}
