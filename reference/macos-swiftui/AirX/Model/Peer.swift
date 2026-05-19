//
//  Peer.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-06-28.
//

import Foundation

public class Peer: Identifiable {
    let hostName: String
    let host: String
    let port: UInt16
    
    public static let sample = Peer(hostName: "Shijunyi", host: "192.168.0.2", port: 9819)
    
    public var description: String {
        return "\(hostName) (\(host):\(port))"
    }
    
    public init(hostName: String, host: String, port: UInt16) {
        self.hostName = hostName
        self.host = host
        self.port = port
    }
    
    public static func fromUid(uid: Int) -> Peer {
        return Peer(hostName: "UID=\(uid)", host: "AnyDrop Kafka Cluster", port: 0)
    }
    
    /// Peer format: <hostname>@<host>:<port>
    public static func parse(_ s: String) -> Optional<Peer> {
        // Incomplete peer string?
        var peerString = s;
        if !peerString.contains("@") {
            peerString = "<empty>@" + s;
        }
        let part1 = peerString.split(separator: "@")
        guard part1.count == 2 else {
            return .none
        }
        
        let part2 = part1[1].split(separator: ":")
        guard part2.count == 2 else {
            return .none
        }
        
        let hostname = part1[0]
        let host = part2[0]
        let port = UInt16(part2[1])
        return .some(Peer(hostName: String(hostname), host: String(host), port: port!))
    }
}
