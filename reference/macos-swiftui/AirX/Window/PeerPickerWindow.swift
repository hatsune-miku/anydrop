//
//  PeerPickerWindow.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-07-14.
//

import Foundation
import AppKit
import SwiftUI

class PeerPickerWindow: NSWindow {
    init(callback: Binding<(Peer) -> Void>) {
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: 328, height: 328),
            styleMask: [.titled, .closable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        center()
        makeKeyAndOrderFront(nil)
        title = "Peer Picker"
        
        contentView = NSHostingView(
            rootView: PeerPickerView(callback: callback))
    }
}
