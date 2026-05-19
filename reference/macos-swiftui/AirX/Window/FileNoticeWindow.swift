//
//  FileNoticeWindow.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-07-14.
//

import Foundation
import AppKit
import SwiftUI

class FileNoticeWindow: NSWindow {
    init(fileId: UInt8) {
        // let frame = NSScreen.screens.first?.visibleFrame
        // let screenWidth = frame?.size.width ?? 0
        // let screenheight = frame?.size.height ?? 0
        
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: 317, height: 196),
            styleMask: [.titled, .closable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        
        center()
        makeKeyAndOrderFront(nil)
        title = "File Notice"
        
        if let receivingFile = GlobalState.shared.receiveFiles[fileId] {
            contentView = NSHostingView(
                rootView: FileNoticeView(receivingFile: receivingFile))
        }
        else {
            contentView = NSHostingView(rootView: Text("Error"))
        }
    }
}
