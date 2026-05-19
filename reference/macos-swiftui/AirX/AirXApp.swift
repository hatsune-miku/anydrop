//
//  SwiftUIPracticeApp.swift
//  SwiftUIPractice
//
//  Created by Hatsune Miku on 2023-01-28.
//

import SwiftUI

@main
struct AnyDropApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self)
    var appDelegate
    
    @Environment(\.openWindow)
    var openWindow
    
    @ObservedObject
    var globalState = GlobalState.shared
    
    private let fileWriterWorker = FileWriterWorker()

    
    private func viewWillAppear() {
        AccountUtils.subscribeToAutomaticLoginResult(id: "default", handler: onAutomaticSignInResult)
        AnyDropService.subscribeToTextChange(id: "default", handler: onTextReceived)
        AnyDropService.subscribeToFileComing(id: "default", handler: onFileComing)
        AnyDropService.subscribeToFilePart(id: "default", handler: onFilePart)
        AnyDropService.subscribeToFileSendingProgress(id: "default", handler: onFileSendingProgress)
        fileWriterWorker.start()
    }
    
    private func onFileSendingProgress(_ fileId: UInt8, _ progress: UInt64, _ total: UInt64, status: FileSendingStatus) {
        // TODO:
        print("fileid=\(fileId), progress=\(progress)/\(total)")
    }
    
    /// Return true to interrupt the connection.
    private func onFilePart(_ fileId: UInt8, _ offset: UInt64, _ length: UInt64, _ data: Data) -> Bool {
        guard let file = globalState.receiveFiles[fileId] else {
            debugPrint("Unexpected file received.")
            return true
        }
        
        guard file.status != .cancelledBySender && file.status != .cancelledByReceiver else {
            debugPrint("File cancelled.")
            return true
        }
        
        fileWriterWorker.addWorkload(
            FileWriterWorker.FilePartWorkload(
                fileId: fileId, offset: offset, length: length, data: data))
        return false
    }
    
    private func onFileComing(_ fileSize: UInt64, _ remoteFullPath: String, _ peer: Peer) {
        let fileName = FileUtils.getFileName(fullPath: remoteFullPath)
        DispatchQueue.main.async {
            let selection = UIUtils.alertBox(
                title: "Received File",
                message: "\(peer.description) is sending \(fileName) (\(fileSize) Bytes) to you!",
                primaryButtonText: "Accept",
                secondaryButtonText: "Explicitly Decline",
                thirdButtonText: "Ignore"
            )
            
            guard selection != .alertThirdButtonReturn else {
                return
            }
            
            let accept = selection == .alertFirstButtonReturn
            let fileId = FileUtils.getNextFileId()
            
            if accept {
                prepareForReceivingFile(fileId, fileSize, remoteFullPath, peer)
            }
            
            AnyDropService.respondToFile(
                host: peer.host, fileId: fileId, fileSize: fileSize, remoteFullPath: remoteFullPath, accept: accept)
        }
    }
    
    private func onTextReceived(_ text: String, _ from: Peer) {
        // These codes must be delayed - they can't run directly during a view update,
        // because `onTextReceived` is called from another thread.
        DispatchQueue.main.async {
            TextNoticeViewModel.shared.receivedText = text
            TextNoticeViewModel.shared.from = from
            TextNoticeViewModel.shared.showTextNotice = true
            UIUtils.createWindow(openWindow, windowId: .textNotice)
        }
    }
    
    // ===========================================================
    
    private func prepareForReceivingFile(_ fileId: UInt8, _ fileSize: UInt64, _ remoteFullPath: String, _ from: Peer) {
        let savingDirectory = FileUtils.getDownloadDirectoryUrl()
            .appending(path: "AnyDropFiles", directoryHint: .isDirectory)
        let fileName = FileUtils.getFileName(fullPath: remoteFullPath)
        let savingFullPath = savingDirectory
            .appending(path: fileName)
        let savingDirectoryPath = savingDirectory.path(percentEncoded: false)
        
        if !FileManager.default.fileExists(
            atPath: savingDirectoryPath) {
            guard let _ = try? FileManager.default.createDirectory(
                atPath: savingDirectoryPath,
                withIntermediateDirectories: true
            ) else {
                _ = UIUtils.alertBox(title: "Error", message: "Can't create output directory \(savingDirectoryPath)", primaryButtonText: "OK")
                return
            }
        }
        
        if !FileManager.default.fileExists(atPath: savingFullPath.path(percentEncoded: false)) {
            guard FileManager.default.createFile(atPath: savingFullPath.path(percentEncoded: false), contents: nil) else {
                DispatchQueue.main.async {
                    _ = UIUtils.alertBox(title: "Error", message: "Can't create file \(savingFullPath.path(percentEncoded: false)) for writing.", primaryButtonText: "OK")
                }
                return
            }
        }
        
        guard let fileHandle = try? FileHandle(forWritingTo: savingFullPath) else {
            DispatchQueue.main.async {
                _ = UIUtils.alertBox(title: "Error", message: "Can't open file \(savingFullPath.path(percentEncoded: false)) for writing.", primaryButtonText: "OK")
            }
            return
        }
        
        guard fileHandle.ensureSize(fileSize) else {
            DispatchQueue.main.async {
                _ = UIUtils.alertBox(title: "Error", message: "Failed to ensure file size.", primaryButtonText: "OK")
            }
            return
        }
        
        let receiveFile = ReceiveFile(
            remoteFullPath: remoteFullPath,
            fileHandle: fileHandle,
            localSaveFullPath: savingFullPath,
            totalSize: fileSize,
            fileId: fileId,
            from: from,
            progress: 0,
            status: .accepted
        )

        GlobalState.shared.receiveFiles[fileId] = receiveFile
        let window = FileNoticeWindow(fileId: fileId)
        UIUtils.showNSWindow(window)
    }
    
    private func onAutomaticSignInResult(didLoginSuccess: Bool) {
        if didLoginSuccess {
            return
        }
        
        // Clear token
        AccountUtils.clearSavedUserInfoAndSignOut()
        
        // Open sign-in window if automatic login failed.
        onToggleSignInOutMenuItemClicked()
    }
    
    var menuItems: some View {
        VStack {
            Group {
                globalState.isServiceOnline
                    ? Text("AnyDrop Is Online!")
                    : Text("AnyDrop Is Offline.")
                
                Button("\(globalState.isServiceOnline ? "Stop" : "Start") Service", action: onToggleServiceMenuItemClicked)
                    .keyboardShortcut("S")
                Button("Open Control Panel", action: onOpenControlPanelMenuItemClicked)
                    .keyboardShortcut("O")
            }

            Divider()
            
            if globalState.isSignedIn {
                Text("UID: \(Defaults.string(.savedUsername, def: "0"))")
            }
            
            Button("Sign \(globalState.isSignedIn ? "Out" : "In")", action: onToggleSignInOutMenuItemClicked)
            
            Divider()
            
            Button("Send File", action: onSendFileMenuItemClicked)
            
            Divider()

            Button("About AnyDrop", action: onAboutMenuItemClicked)
                .keyboardShortcut("A")

            Button("Exit", action: onExitApplicationMenuItemClicked)
                .keyboardShortcut("E")
        }
    }
    
    var exitingMenuItems: some View {
        VStack {
            Text("AnyDrop is exiting...")
            Button("Force Exit", action: onForceExitMenuItemClicked)
                .keyboardShortcut("F")
        }
    }
    
    var body: some Scene {
        let _ = viewWillAppear()

        MenuBarExtra("AnyDrop", image: "AppIconBoldTransparent") {
            if globalState.isApplicationExiting {
                exitingMenuItems
            }
            else {
                menuItems
            }
        }

        Window(WindowIds.signIn.windowTitle, id: WindowIds.signIn.rawValue) {
            LoginView(isSignedInRef: $globalState.isSignedIn)
        }.windowResizability(.contentSize)
            .defaultPosition(.center)
            .defaultSize(width: 366, height: 271)

        Window(WindowIds.controlPanel.windowTitle, id: WindowIds.controlPanel.rawValue) {
            ControlPanelView()
        }.windowResizability(.contentSize)
            .defaultPosition(.center)
            .defaultSize(width: 277, height: 460)
        
        Window(WindowIds.about.windowTitle, id: WindowIds.about.rawValue) {
            AboutView()
        }.windowResizability(.contentSize)
            .defaultPosition(.center)
            .defaultSize(width: 305, height: 273)
        
        Window(WindowIds.textNotice.windowTitle, id: WindowIds.textNotice.rawValue) {
            TextNoticeView(theme: .constant(LightMode()))
        }.windowResizability(.contentSize)
            .windowStyle(.hiddenTitleBar)
            .defaultPosition(.bottomTrailing)
            .defaultSize(width: 317, height: 196)
    }
    
    func onToggleServiceMenuItemClicked() {
        if globalState.isServiceOnline {
            AnyDropService.initiateStopAsync()
        }
        else {
            AnyDropService.startAsync()
        }
    }
    
    func onAboutMenuItemClicked() {
        UIUtils.createWindow(openWindow, windowId: .about)
    }
    
    func onToggleSignInOutMenuItemClicked() {
        if globalState.isSignedIn {
            globalState.isSignedIn = false
            AccountUtils.clearSavedUserInfoAndSignOut()
            return
        }
        UIUtils.createWindow(openWindow, windowId: .signIn)
    }
    
    func onOpenControlPanelMenuItemClicked() {
        UIUtils.createWindow(openWindow, windowId: .controlPanel)
    }
    
    func onExitApplicationMenuItemClicked() {
        globalState.isApplicationExiting = true
        AnyDropService.initiateStopAsync()
        Task {
            do {
                // TODO: wait for stop
                try await Task.sleep(for: .seconds(2))
            }
            catch {}
            exit(EXIT_SUCCESS)
        }
    }
    
    func onSendFileMenuItemClicked() {
        let peers = AnyDropService.readCurrentPeers()
        guard !peers.isEmpty else {
            _ = UIUtils.alertBox(title: "Error", message: "No peers available.", primaryButtonText: "OK")
            return
        }
        
        guard let fileUrl = UIUtils.pickFile() else {
            return
        }
        
        let peerPicker = PeerPickerWindow(callback: .constant({ peer in
            debugPrint("Sending \(fileUrl.path(percentEncoded: false))")
            AnyDropService.trySendFile(host: peer.host, filePath: fileUrl.path(percentEncoded: false))
        }))
        UIUtils.showNSWindow(peerPicker)
    }
    
    func onForceExitMenuItemClicked() {
        exit(EXIT_FAILURE)
    }
}

enum WindowIds: String {
    case signIn
    case controlPanel
    case about
    case textNotice
    case fileNotice
    
    var windowTitle: String {
        switch self {
        case .signIn:
            return "Sign In"
            
        case .controlPanel:
            return "Developer Control Panel"
            
        case .about:
            return "About AnyDrop"
            
        case .textNotice:
            return "Text Notice"
            
        case .fileNotice:
            return "File Notice"
        }
    }
}
