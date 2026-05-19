//
//  AppDelegate.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-06-14.
//

import AppKit
import Foundation
import SwiftUI
import GoogleSignIn

class AppDelegate: NSResponder, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        preventMultipleInstances()
        
        anydrop_init()
        Defaults.tryInitializeConfigurationsForFirstRun()
        registerForGoogleSignIn()
        AccountUtils.tryLoginWithSavedToken()
        AnyDropService.startAsync()

        print("AnyDrop macOS Frontend")
    }

    // Keep alive in background.
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        return false
    }
    
    func registerForGoogleSignIn() {
        // Register for GetURL events.
        let appleEventManager = NSAppleEventManager.shared()
        appleEventManager.setEventHandler(
            self,
            andSelector: #selector(handleGetURLEvent),
            forEventClass: AEEventClass(kInternetEventClass),
            andEventID: AEEventID(kAEGetURL)
        )
    }
    
    private func preventMultipleInstances() {
        let myPid = ProcessInfo.processInfo.processIdentifier
        let workspace = NSWorkspace.shared
        let apps = workspace.runningApplications
        
        for app in apps {
            if app.processIdentifier != myPid && app.bundleIdentifier == Bundle.main.bundleIdentifier {
                print("Another instance is already running. Exiting...")
                exit(EXIT_SUCCESS)
            }
        }
    }
    
    @objc func handleGetURLEvent(event: NSAppleEventDescriptor?, replyEvent: NSAppleEventDescriptor?) {
        if let urlString = event?.paramDescriptor(forKeyword: AEKeyword(keyDirectObject))?.stringValue{
            let url = URL(string: urlString)
            GIDSignIn.sharedInstance.handle(url!)
        }
    }
}
