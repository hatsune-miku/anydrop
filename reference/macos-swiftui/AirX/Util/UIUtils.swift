//
//  UIUtil.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-06-14.
//

import Foundation
import AppKit
import SwiftUI

class UIUtils {
    public static func hideDockIcons() {
        NSApplication.shared.setActivationPolicy(.prohibited)
    }
    
    public static func showDockIcons() {
        NSApplication.shared.setActivationPolicy(.regular)
    }
    
    public static func createWindow(_ openWindowAction: OpenWindowAction, windowId: WindowIds) {
        showDockIcons()
        openWindowAction(id: windowId.rawValue)
        
        // 啊？
        NSApplication.activate(NSApplication.shared)
            .self(ignoringOtherApps: true)
    }
    
    public static func showNSWindow(_ window: NSWindow) {
        let controller = NSWindowController()
        controller.contentViewController = window.contentViewController
        controller.window = window
        controller.showWindow(self)
    }
    
    public static func alertBox(
        title: String,
        message: String,
        primaryButtonText: String,
        secondaryButtonText: String? = nil,
        thirdButtonText: String? = nil
    ) -> NSApplication.ModalResponse {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = message
        alert.addButton(withTitle: primaryButtonText)
        
        if let secondaryButtonText {
            alert.addButton(withTitle: secondaryButtonText)
        }
        
        if let thirdButtonText {
            alert.addButton(withTitle: thirdButtonText)
        }
        
        alert.alertStyle = .informational
        return alert.runModal()
    }
    
    public static func pickFile() -> Optional<URL> {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        guard panel.runModal() == .OK else {
            return .none
        }
        return panel.url
    }
}
