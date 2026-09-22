//! Generates interoperability fixtures from the existing desktop text encoder.
use anydrop::packet::data::magic_numbers::MagicNumbers;
use anydrop::packet::data::text_packet::TextPacket;
use anydrop::packet::data_packet::DataPacket;
use anydrop::packet::protocol::serialize::Serialize;
fn main() {
    let samples = [
        "AnyDrop",
        "你好\n同频段文本",
        "e\u{301} 👩‍💻 😀",
        "\0\t\r\n",
        "a",
    ];
    let fixtures: Vec<_> = samples.iter().map(|text| {
        let text_packet = TextPacket::new((*text).to_string()).unwrap().serialize();
        let packet = DataPacket::new(MagicNumbers::Text.value(), &text_packet).serialize();
        let mut framed = (packet.len() as u32).to_le_bytes().to_vec();
        framed.extend_from_slice(&packet);
        serde_json::json!({"text": text, "hex": framed.iter().map(|byte| format!("{byte:02x}")).collect::<String>()})
    }).collect();
    println!("{}", serde_json::to_string_pretty(&fixtures).unwrap());
}
