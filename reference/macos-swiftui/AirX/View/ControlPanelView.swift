//
//  PanelView.swift
//  SwiftUIPractice
//
//  Created by Hatsune Miku on 2023-忘了.
//

import SwiftUI

struct ControlPanelView: View {
    @State var peers: [Peer] = []
    @State var timer: Timer?
    @State private var selectedMode: ThemeMode = .light
    @ObservedObject var clipboardMonitor = TextNoticeViewModel.shared
    @ObservedObject var globalState = GlobalState.shared
    
    var buffer = UnsafeMutablePointer<CChar>
        .allocate(capacity: 4096)
    
    func startRefreshingPeerList() {
        timer = .scheduledTimer(
            withTimeInterval: 0.5,
            repeats: true,
            block: { _ in
                peers = AnyDropService.readCurrentPeers()
            }
        )
    }
    
    func stopRefreshingPeerList() {
        timer?.invalidate()
        timer = .none
    }
    
    func onButtonClicked() {
        if globalState.isServiceOnline {
            globalState.isServiceOnline = false
            AnyDropService.initiateStopAsync()
            stopRefreshingPeerList()
        }
        else {
            globalState.isServiceOnline = true
            AnyDropService.startAsync()
            startRefreshingPeerList()
        }
    }
    
    
    var body: some View {
        VStack {
            HStack {
                Image("AppIconBoldTransparent")
                Text("AnyDrop Developer Control Panel").bold()
            }
            
            Divider()
            
            Group {
                Text("- Peer List -")

                if peers.isEmpty {
                    Text("(Empty)").bold()
                }
                else {
                    // Note: Items in foreach arrays should implement Identifiable
                    ForEach(peers) { peer in
                        Text(peer.description)
                    }
                }
            }
            
            Divider()
            
            Button(
                globalState.isServiceOnline ? "Stop Service" : "Start Service"
                   , action: onButtonClicked)
                .foregroundColor(selectedMode.theme.buttonText)
            
            Spacer().frame(height: 20)
            
            HStack {
                Text("Mode")
                    .font(.footnote)
                    .foregroundColor(selectedMode.theme.textColor) // 设置颜色
                
                Spacer().frame(width: 5)
                
                Picker("", selection: $selectedMode) {
                    ForEach(ThemeMode.allCases, id: \.self) {
                        Text($0.rawValue).font(.footnote)
                    }
                }
                //.pickerStyle(RadioGroupPickerStyle())
                .foregroundColor(selectedMode.theme.textColor)
                .background(selectedMode.theme.gray)
                .frame(width: 130, height: 30)
            }
        }
        .frame(width: 245, height: 400)
        .padding()
        .background(selectedMode.theme.gray)
    }
}

struct PanelView_Previews: PreviewProvider {
    static var previews: some View {
        ControlPanelView()
    }
}
