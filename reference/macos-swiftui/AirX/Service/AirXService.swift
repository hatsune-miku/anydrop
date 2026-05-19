//
//  AnyDropService.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-06-14.
//

import Foundation
import AppKit

// Global interrupt flag.
private var interrupt = false
private var pasteboard = NSPasteboard.general
private var lastIncomingString = ""
private var textChangeSubscribers = Dictionary<String, (String, Peer) -> Void>()
private var fileSendingProgressSubscribers = Dictionary<String, (UInt8, UInt64, UInt64, FileSendingStatus) -> Void>();
private var fileComingSubscribers = Dictionary<String, (UInt64, String, Peer) -> Void>();
private var filePartSubscribers = Dictionary<String, (UInt8, UInt64, UInt64, Data) -> Bool>();

private func shouldInterrupt() -> Bool {
    return interrupt
}

/// Return true to interrupt the transmission.
private func onFilePart(
    fileId: UInt8,
    offset: UInt64,
    length: UInt64,
    data: UnsafePointer<UInt8>?
) -> Bool {
    guard let data else {
        return false
    }
    
    let dataManaged = Data(bytes: data, count: Int(length))

    for subscriber in filePartSubscribers.values {
        if subscriber(fileId, offset, length, dataManaged) {
            return true
        }
    }
    return false
}

private func onFileSendingProgress(
    fileId: UInt8,
    progress: UInt64,
    total: UInt64,
    status: UInt8
) {
    for subscriber in fileSendingProgressSubscribers.values {
        subscriber(fileId, progress, total, .init(rawValue: status) ?? .requested)
    }
}

private func onFileComing(
    fileSize: UInt64,
    fileNameStringPointer: UnsafePointer<CChar>?,
    fileNameStringLength: UInt32,
    sourceIpAddressStringPointer: UnsafePointer<CChar>?,
    sourceIpAddressStringLength: UInt32
) {
    guard let fileNameStringPointer, let sourceIpAddressStringPointer else {
        return
    }
    
    let fileName = String(cString: fileNameStringPointer, length: Int(fileNameStringLength))
    let sourceIpAddressString = String(cString: sourceIpAddressStringPointer, length: Int(sourceIpAddressStringLength))
    
    guard let fileName, let sourceIpAddressString else {
        return
    }
    
    let peer = Peer.parse(sourceIpAddressString)
    
    guard let peer else {
        print("Peer parsing failed.")
        return
    }
    
    for subscriber in fileComingSubscribers.values {
        subscriber(fileSize, fileName, peer)
    }
}

public func onTextReceived(
    incomingString: String,
    peer: Peer
) {
    // Myself?
    if let currentContent = pasteboard.string(forType: .string) {
        if currentContent == incomingString {
            print("Text received from myself. Ignored.")
            return
        }
    }

    // Copy string to pasteboard.
    pasteboard.declareTypes([.string], owner: nil)
    lastIncomingString = incomingString
    guard pasteboard.setString(incomingString, forType: .string) else {
        return
    }
    
    for subscriber in textChangeSubscribers.values {
        subscriber(incomingString, peer)
    }
}

private func onTextReceived(
    incomingStringPointer: UnsafePointer<CChar>?,
    incomingStringLength: UInt32,
    sourceIpAddressStringPointer: UnsafePointer<CChar>?,
    sourceIpAddressStringLength: UInt32
) {
    guard let incomingStringPointer, let sourceIpAddressStringPointer else {
        return
    }
    
    let incomingString = String(cString: incomingStringPointer, length: Int(incomingStringLength))
    let sourceIpAddressString = String(cString: sourceIpAddressStringPointer, length: Int(sourceIpAddressStringLength))
    
    guard let incomingString, let sourceIpAddressString else {
        return
    }
    
    let peer = Peer.parse(sourceIpAddressString)
    
    guard let peer else {
        print("Peer parsing failed.")
        return
    }
    
    onTextReceived(incomingString: incomingString, peer: peer)
}

class AnyDropService {
    // AnyDrop service opaque structure.
    private static var anydropPointer: OpaquePointer?  = .none

    // Pasteboard last change count and content.
    private static var lastPasteboardChangeCount    = -1
    private static var lastPasteboardContent        = ""
    
    // Timer for monitoring the clipboard.
    private static var timerClipboardMonitor: Timer? = .none

    private static var timerAutoPeerRefresh: Timer? = .none
    
    // Workers.
    private static var threads: Array<Thread>       = .init()

    // Configurations.
    public static let discoveryServiceServerPort   = Defaults.int(.discoveryServiceServerPort)
    public static let discoveryServiceClientPort   = Defaults.int(.discoveryServiceClientPort)
    public static let textServiceListenPort        = Defaults.int(.textServiceListenPort)
    public static let groupIdentity                = Defaults.int(.groupIdentity)
    public static let host                         = "0.0.0.0"

    public static func subscribeToTextChange(id: String, handler: @escaping (String, Peer) -> Void) {
        textChangeSubscribers[id] = handler
    }
    
    public static func subscribeToFileSendingProgress(id: String, handler: @escaping (UInt8, UInt64, UInt64, FileSendingStatus) -> Void) {
        fileSendingProgressSubscribers[id] = handler
    }
    
    public static func subscribeToFilePart(id: String, handler: @escaping (UInt8, UInt64, UInt64, Data) -> Bool) {
        filePartSubscribers[id] = handler
    }
    
    public static func subscribeToFileComing(id: String, handler: @escaping (UInt64, String, Peer) -> Void) {
        fileComingSubscribers[id] = handler
    }
    
    public static func startAsync() {
        guard threads.isEmpty else {
            return
        }
        
        let hostBuffer = host.toBuffer()
        anydropPointer = anydrop_create(
            UInt16(discoveryServiceServerPort),
            UInt16(discoveryServiceClientPort),
            hostBuffer,
            host.utf8Size(),
            UInt16(textServiceListenPort),
            UInt32(groupIdentity)
        )
        hostBuffer.deallocate()
        
        // Run text and lan discovery service in seperate threads.
        threads.append(Thread(block: {
            anydrop_data_service(
                anydropPointer,
                onTextReceived,
                onFileComing,
                onFileSendingProgress,
                onFilePart,
                shouldInterrupt
            )
        }))
        threads.append(Thread(block: {
            anydrop_lan_discovery_service(anydropPointer, shouldInterrupt)
        }))
        
        // Reset interrupt flag and start the services!
        interrupt = false
        for t in threads {
            t.start()
        }
        startMonitoringClipboard()
        startAutoRefreshPeers()
        GlobalState.shared.isServiceOnline = true
    }

    // Services stop at their next ticks.
    public static func initiateStopAsync() {
        interrupt = true
        for t in threads {
            t.cancel()
        }
        threads.removeAll()
        stopMonitoringClipboard()
        stopAutoRefreshPeers()
        GlobalState.shared.isServiceOnline = false
    }
    
    public static func readCurrentPeers() -> [Peer] {
        let buffer = UnsafeMutablePointer<CChar>
            .allocate(capacity: 4096)
        let len = Int(anydrop_get_peers(anydropPointer, buffer))

        // 简单封个口
        buffer.advanced(by: len).update(repeating: 0, count: 1)
        
        // Decode and free.
        defer { buffer.deallocate() }
        return String(cString: buffer)
            .split(separator: .init(unicodeScalarLiteral: ","))
            .map({ substring in Peer.parse(String(substring))! })
    }
    
    public static func trySendFile(host: String, filePath: String) {
        let filePathUrlDecoded = filePath.removingPercentEncoding ?? filePath
        
        let hostBuffer = host.toBuffer()
        let pathBuffer = filePathUrlDecoded.toBuffer()
        
        anydrop_try_send_file(
            anydropPointer!,
            hostBuffer,
            host.utf8Size(),
            pathBuffer,
            filePathUrlDecoded.utf8Size()
        )
    }
    
    public static func respondToFile(host: String, fileId: UInt8, fileSize: UInt64, remoteFullPath: String, accept: Bool) {
        let hostBuffer = host.toBuffer()
        let remoteFullPathBuffer = remoteFullPath.toBuffer()
        
        anydrop_respond_to_file(
            anydropPointer!,
            hostBuffer,
            host.utf8Size(),
            fileId,
            fileSize,
            remoteFullPathBuffer,
            remoteFullPath.utf8Size(),
            accept
        )
    }
    
    public static func readVersionString() -> String {
        let buffer = UnsafeMutablePointer<CChar>
            .allocate(capacity: 128)
        let len = Int(anydrop_version_string(buffer))
        
        // Ensure zero terminated
        buffer.advanced(by: len).update(repeating: 0, count: 1)
        defer { buffer.deallocate() }
        return String(cString: buffer)
    }
    
    private static func startMonitoringClipboard() {
        stopMonitoringClipboard()
        timerClipboardMonitor = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { _ in
            if self.lastPasteboardChangeCount != pasteboard.changeCount {
                self.lastPasteboardChangeCount = pasteboard.changeCount
                if let newContent = pasteboard.string(forType: .string), newContent != self.lastPasteboardContent {
                    self.lastPasteboardContent = newContent
                    self.onPasteboardChanged(newContent: newContent)
                }
            }
        }
    }
    
    private static func stopMonitoringClipboard() {
        timerClipboardMonitor?.invalidate()
        timerClipboardMonitor = nil
    }
    
    private static func startAutoRefreshPeers() {
        stopAutoRefreshPeers()
        timerAutoPeerRefresh = Timer.scheduledTimer(withTimeInterval: 10, repeats: true) { _ in
            guard anydropPointer != .none else {
                return
            }
            anydrop_lan_broadcast(anydropPointer)
        }
    }
    
    private static func stopAutoRefreshPeers() {
        timerAutoPeerRefresh?.invalidate()
        timerAutoPeerRefresh = nil
    }
    
    private static func onPasteboardChanged(newContent: String) {
        guard newContent != lastIncomingString else {
            // Prevent recursive copy-send.
            print("Prevented recursive copy-send.")
            return
        }

        print("Clipboard changed, broadcasting new text to LAN.")
        let buffer = newContent.toBuffer()
        anydrop_broadcast_text(anydropPointer!, buffer, newContent.utf8Size())
        buffer.deallocate()

        if GlobalState.shared.isSignedIn {
            print("Clipboard changed, broadcasting new text to inet.")
            try? AnyDropCloud.sendMessage(content: newContent, type: .text, completion: { response in
                print(response.message)
            })
        }
    }
}
