//
//  ReceiveFile.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-07-14.
//

import Foundation

class ReceiveFile: ObservableObject {
    @Published var remoteFullPath: String
    @Published var fileHandle: FileHandle
    @Published var localSaveFullPath: URL
    @Published var totalSize: UInt64
    @Published var fileId: UInt8
    @Published var from: Peer
    @Published var progress: UInt64
    @Published var status: FileSendingStatus
    
    public var sizeRepresentation: String {
        let units = ["Bytes", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB", "啊？", "nb"]
        var unitIndex = 0
        var size = Double(totalSize)
        
        while size > 1024 && unitIndex < units.count - 1 {
            size /= 1024
            unitIndex += 1
        }

        // Use %.2f to keep two decimal places
        return String(format: "%.2f \(units[unitIndex])", size)
    }
    
    init(remoteFullPath: String, fileHandle: FileHandle, localSaveFullPath: URL, totalSize: UInt64, fileId: UInt8, from: Peer, progress: UInt64, status: FileSendingStatus) {
        self.remoteFullPath = remoteFullPath
        self.fileHandle = fileHandle
        self.localSaveFullPath = localSaveFullPath
        self.totalSize = totalSize
        self.fileId = fileId
        self.from = from
        self.progress = progress
        self.status = status
    }
    
    public static let sample = ReceiveFile(
        remoteFullPath: "sample.pdf",
        fileHandle: FileHandle(),
        localSaveFullPath: .downloadsDirectory,
        totalSize: 11451419198106660,
        fileId: 255,
        from: .sample,
        progress: 80000,
        status: .inProgress
    )
}
