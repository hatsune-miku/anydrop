//
//  WebSocketService.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-07-22.
//

import Foundation
import Starscream

class WebSocketService: WebSocketDelegate {
    var socket: WebSocket?
    
    static let shared = WebSocketService()
    
    func initialize() -> Bool {
        if let socket {
            socket.disconnect(closeCode: 0)
        }
        
        /// Not logged in?
        let uid = Defaults.string(.loggedInUid, def: "")
        if uid.isEmpty {
            return false
        }
        
        var request = URLRequest(url: URL(string: AnyDropCloud.WEBSOCKET_BASE)!)
        request.timeoutInterval = 5

        print("(Re)Connecting to the websocket...")
        socket = WebSocket(request: request)
        socket?.delegate = self
        socket?.connect()

        return true
    }
    
    func onBinaryReceived(data: Data, client: Starscream.WebSocket) {
        guard let message = Message.parse(data: data) else {
            print("Failed to parse incoming data, dropping.")
            return
        }
        print("Message received from \(message.senderUid)")

        switch message.type {
        case .text:
            onTextReceived(
                incomingString: message.rawContent,
                peer: .fromUid(uid: message.senderUid)
            )
            break

        case .fileUrl:
            break
        }
    }
    
    func reconnect() {
        if Self.shared.initialize() {
            return
        }
        
        print("Reconnection failed.")
        DispatchQueue.main.async {
            _ = UIUtils.alertBox(
                title: "WebSocket Error",
                message: "Failed to reconnect with backend registration center. Text & sharing over inet will not work.",
                primaryButtonText: "OK"
            )
        }
    }
    
    func registerDevice() {
        let uid = Defaults.string(.loggedInUid, def: "")
        socket?.write(string: uid)
        print("Tried to register with backend.")
    }
    
    func didReceive(event: Starscream.WebSocketEvent, client: Starscream.WebSocket) {
        switch event {
        case .connected(_):
            print("WebSocket connected.")
            registerDevice()
            break
            
        case .disconnected(_, _):
            print("Websocket disconnected.")
            reconnect()
            break
            
        case .text(let string):
            print("Unexpected text message received: (\(string))")
            break
            
        case .binary(let data):
            onBinaryReceived(data: data, client: client)
            break
            
        case .reconnectSuggested(let suggested):
            if suggested {
                reconnect()
            }
            break
            
        case .error(let e):
            print(e?.localizedDescription ?? "Unknown websocket error")
            
        case .pong(_), .ping(_), .viabilityChanged(_), .cancelled:
            break
        }
    }
}
