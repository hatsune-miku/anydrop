import Foundation

@main struct CheckMobileWire {
  static func main() throws {
    let data = try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1]))
    let fixtures = try JSONSerialization.jsonObject(with: data) as! [[String: String]]
    func rejected(_ operation: () throws -> Void) {
      do {
        try operation()
        fatalError("Malformed frame was accepted")
      } catch {}
    }
    for fixture in fixtures {
      let text = fixture["text"]!
      let frame = try TextWire.encode(text)
      precondition(frame.map { String(format: "%02x", $0) }.joined() == fixture["hex"])
      let decoded = try TextWire.decode(frame)
      precondition(decoded == text)
      for length in 0..<frame.count {
        rejected { _ = try TextWire.decode(Data(frame.prefix(length))) }
      }
      for index in [0, 4, 6, 10, frame.count - 1] {
        var corrupt = frame
        corrupt[index] ^= 0x80
        rejected { _ = try TextWire.decode(corrupt) }
      }
    }
    var invalidUTF8 = try TextWire.encode("a")
    invalidUTF8[14] = 255
    rejected { _ = try TextWire.decode(invalidUTF8) }
    let large = String(repeating: "a", count: 65535)
    let decodedLarge = try TextWire.decode(TextWire.encode(large))
    precondition(decodedLarge == large)
    rejected { _ = try TextWire.encode(String(repeating: "a", count: 65536)) }
    rejected { _ = try TextWire.encode("") }
    print(
      "Swift text protocol: desktop fixtures, truncated frames, corrupt headers, Unicode and limits passed"
    )
  }
}
