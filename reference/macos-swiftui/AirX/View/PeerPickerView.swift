//
//  PeerPickerView.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-07-14.
//

import Foundation
import SwiftUI


struct PeerPickerView: View {
    @Environment(\.presentationMode) var presentationMode
    
    @Binding var callback: (Peer) -> Void
    
    let peers = AnyDropService.readCurrentPeers()

    var body: some View {
        VStack(spacing: 0) {
            Text("Select a peer")
                .bold()
                .padding()

            ForEach(peers) { peer in
                PeerPickerRow(peer: peer) {
                    presentationMode.wrappedValue.dismiss()
                    callback(peer)
                }
            }
        }
        .frame(width: 328)
    }
}

struct PeerPickerRow: View {
    var peer: Peer
    var onClick: () -> Void
    
    var body: some View {
        Button(action: onClick) {
            VStack(alignment: .leading) {
                Text(peer.hostName)
                    .fontWeight(.bold)
                    .font(.system(size: 14))
                Text(peer.description)
            }.padding(4)
            Spacer()
        }
        .padding(8)
    }
}

struct PeerPickerView_Previews: PreviewProvider {
    static var previews: some View {
        PeerPickerView(callback: .constant({ _ in }))
    }
}
