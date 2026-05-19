//
//  StringHash.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-06-14.
//

import Foundation
import CryptoKit

extension Digest {
    var bytes: [UInt8] { Array(makeIterator()) }
    var data: Data { Data(bytes) }

    var hexString: String {
        bytes.map { String(format: "%02x", $0) }.joined()
    }
}

extension String {
    func sha256() -> String? {
        guard let data = self.data(using: .utf8) else {
            return nil
        }
        return SHA256.hash(data: data).hexString
    }
}
