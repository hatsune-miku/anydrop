//
//  Message.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-07-22.
//

import Foundation

// 4 Bytes: senderUid
// 4 Bytes: type
// 4 Bytes: rawContentLength
// N Bytes: rawContent
class Message {
    let senderUid: Int
    let type: MessageType
    let rawContent: String
    
    init(senderUid: Int, type: MessageType, rawContent: String) {
        self.senderUid = senderUid
        self.type = type
        self.rawContent = rawContent
    }
    
    static func parse(data: Data) -> Message? {
        guard data.count >= 12 else {
            // Data is too short to contain all required components
            return nil
        }
        
        let senderUidData = data[0..<4]
        let typeData = data[4..<8]
        let rawContentLengthData = data[8..<12]
        
        let senderUid = Int(Int32(bigEndian: senderUidData.withUnsafeBytes { $0.load(as: Int32.self) }))
        let typeRawValue = Int(Int32(bigEndian: typeData.withUnsafeBytes { $0.load(as: Int32.self) }))
        guard let type = MessageType(rawValue: typeRawValue) else {
            // Invalid MessageType
            return nil
        }
        
        let rawContentLength = Int(Int32(bigEndian: rawContentLengthData.withUnsafeBytes { $0.load(as: Int32.self) }))
        guard data.count >= 12 + rawContentLength else {
            // Data is too short to contain content
            return nil
        }
        
        let rawContentData = data[12..<(12 + rawContentLength)]
        guard let rawContent = String(data: rawContentData, encoding: .utf8) else {
            // Invalid String data
            return nil
        }
        
        return Message(senderUid: senderUid, type: type, rawContent: rawContent)
    }
}

enum MessageType: Int {
    case text = 1
    case fileUrl = 2
}
