//
//  FileHandleEnsureLength.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-07-14.
//

import Foundation

extension FileHandle {
    func ensureSize(_ size: UInt64) -> Bool {
        guard let _ = try? seekToEnd() else {
            return false
        }

        var bytesWritten = 0
        while bytesWritten < size {
            let chunkSize = min(32768, Int(size) - bytesWritten)
            let chunk = Data(count: chunkSize)
            guard let _ = try? write(contentsOf: chunk) else {
                return false
            }
            bytesWritten += chunkSize
        }
        
        guard let _ = try? seek(toOffset: 0) else {
            return false
        }
        return true
    }
}
