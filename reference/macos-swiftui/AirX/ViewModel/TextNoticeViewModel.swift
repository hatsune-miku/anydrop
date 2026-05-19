//
//  ClipboardMonitor.swift
//  SwiftUIPractice
//
//  Created by 刘世俊懿 on 2023-05-21.
//

import Foundation
import SwiftUI
import Combine
import AppKit

class TextNoticeViewModel: ObservableObject {
    @Published var showTextNotice: Bool = false
    @Published var receivedText: String = "你好"
    @Published var from: Peer = .sample
    
    static let shared = TextNoticeViewModel()
}
