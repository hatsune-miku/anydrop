import Foundation

enum TextWire {
  static func u16(_ bytes: Data, _ start: Int) -> Int {
    Int(bytes[start]) | (Int(bytes[start + 1]) << 8)
  }
  static func u32(_ bytes: Data, _ start: Int) -> Int {
    u16(bytes, start) | (u16(bytes, start + 2) << 16)
  }
  static func hash(_ text: String) -> Int {
    text.unicodeScalars.enumerated().reduce(65526) {
      ($0 + $1.offset * Int($1.element.value)) & 65535
    }
  }
  static func encode(_ text: String) throws -> Data {
    let bytes = Data(text.utf8)
    guard !bytes.isEmpty, bytes.count <= 65535 else {
      throw NSError(
        domain: "AnyDrop", code: 2, userInfo: [NSLocalizedDescriptionKey: "文本必须为 1 到 65535 字节"])
    }
    func little(_ value: Int, _ count: Int) -> Data {
      Data((0..<count).map { UInt8((value >> ($0 * 8)) & 255) })
    }
    let inner = little(bytes.count, 4) + bytes + little(hash(text), 2)
    let packet = little(0x3940, 2) + little(inner.count, 4) + inner + little(inner.count / 2, 2)
    return little(packet.count, 4) + packet
  }
  static func decode(_ bytes: Data) throws -> String {
    func invalid() -> NSError {
      NSError(domain: "AnyDrop", code: 3, userInfo: [NSLocalizedDescriptionKey: "文本数据包无效"])
    }
    guard bytes.count >= 18, u32(bytes, 0) == bytes.count - 4, u16(bytes, 4) == 0x3940 else {
      throw invalid()
    }
    let length = u32(bytes, 6)
    let textSize = u32(bytes, 10)
    guard length == textSize + 6, bytes.count == length + 12, textSize <= 65535,
      u16(bytes, bytes.count - 2) == (length / 2) & 65535,
      let text = String(data: bytes.subdata(in: 14..<(14 + textSize)), encoding: .utf8),
      u16(bytes, 14 + textSize) == hash(text)
    else { throw invalid() }
    return text
  }
}
