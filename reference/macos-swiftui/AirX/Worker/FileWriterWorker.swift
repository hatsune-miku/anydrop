//
//  FileWriterWorker.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-07-14.
//

import Foundation

class FileWriterWorker {
    private let queue = BlockingQueue<FilePartWorkload>()
    private var workerThread: Thread?
    private var shouldInterrupt = false
    
    public func addWorkload(_ workload: FilePartWorkload) {
        queue.enqueue(item: workload)
    }
    
    public func start() {
        guard workerThread == nil else {
            return
        }
        
        workerThread = Thread { [self] in
            while !shouldInterrupt {
                let workload = queue.dequeue()
                handleSingleWorkload(workload)
            }
        }
        workerThread?.start()
    }
    
    public func stop() {
        shouldInterrupt = true
        workerThread = nil
    }
    
    private func handleSingleWorkload(_ workload: FilePartWorkload) {
        guard let file = GlobalState.shared.receiveFiles[workload.fileId] else {
            return
        }
        
        // Cancelled?
        guard file.status != .cancelledByReceiver else {
            debugPrint("File cancelled!")
            try? file.fileHandle.close()
            GlobalState.shared.receiveFiles.removeValue(forKey: workload.fileId)
            return
        }
        
        // In progress.
        do {
            try file.fileHandle.seek(toOffset: workload.offset)
            try file.fileHandle.write(contentsOf: workload.data)
        }
        catch {
            DispatchQueue.main.async {
                file.status = .error
            }
            return
        }
        
        DispatchQueue.main.async {
            file.progress += workload.length
            file.status = .inProgress
        }
        
        // Finished?
        if file.progress == file.totalSize {
            DispatchQueue.main.async {
                file.status = .completed
            }
            try? file.fileHandle.close()
            debugPrint("File receive completed!")
        }
    }
    
    public struct FilePartWorkload {
        public let fileId: UInt8
        public let offset: UInt64
        public let length: UInt64
        public let data: Data
        
        init(fileId: UInt8, offset: UInt64, length: UInt64, data: Data) {
            self.fileId = fileId
            self.offset = offset
            self.length = length
            self.data = data
        }
    }
}
