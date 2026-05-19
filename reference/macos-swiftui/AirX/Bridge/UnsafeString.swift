//
//  UnsafeString.swift
//  SwiftUIPractice
//
//  Created by Hatsune Miku on 2023-01-29.
//

import Foundation

extension String {
    /// Write UTF8 representation of string to a raw buffer.
    func writeTo(buffer: UnsafeMutablePointer<UInt8>) -> Int {
        let data = self.data(using: .utf8)!
        let stream = OutputStream(toBuffer: buffer, capacity: data.count)
        stream.open()
        do {
            try data.withUnsafeBytes({ (p: UnsafeRawBufferPointer) throws -> Void in
                stream.write(
                    p.bindMemory(to: UInt8.self).baseAddress!,
                    maxLength: data.count
                )
            })
        }
        catch {
            
        }
        
        stream.close()
        return data.count
    }
    
    func toBuffer() -> UnsafeMutablePointer<UInt8> {
        let data = self.data(using: .utf8)!
        let buffer = UnsafeMutablePointer<UInt8>.allocate(capacity: data.count)
        _ = self.writeTo(buffer: buffer)
        return buffer
    }
    
    func utf8Size() -> UInt32 {
        return UInt32(self.data(using: .utf8)!.count)
    }
}
