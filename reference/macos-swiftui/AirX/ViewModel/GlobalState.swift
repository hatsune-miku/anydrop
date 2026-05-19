//
//  GlobalState.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-06-15.
//

import Foundation

class GlobalState: ObservableObject {
    @Published var isServiceOnline: Bool = false
    @Published var isSignedIn: Bool = false
    @Published var isApplicationExiting: Bool = false
    @Published var receiveFiles: Dictionary<UInt8, ReceiveFile> = [255: .sample];
    
    static let shared = GlobalState()
}
