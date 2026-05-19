//
//  Defaults.swift
//  SwiftUIPractice
//
//  Created by Hatsune Miku on 2023-06-14.
//

import Foundation

enum DefaultKeys: String {
    case savedUsername
    case shouldRememberPassword

    /// Tokens or password.
    case loggedInUid
    case savedCredentialType
    case savedCredential
    
    case discoveryServiceServerPort
    case discoveryServiceClientPort
    case textServiceListenPort
    case groupIdentity
    
    case isNotFirstRun
}

class Defaults {
    private static let defaults = UserDefaults.standard
    
    public static func tryInitializeConfigurationsForFirstRun() {
        if bool(.isNotFirstRun) {
            return
        }
        
        write(.discoveryServiceClientPort, value: 0)
        write(.discoveryServiceServerPort, value: 9818)
        write(.textServiceListenPort, value: 9819)
        write(.groupIdentity, value: 0)
        write(.isNotFirstRun, value: true)
    }
    
    public static func string(_ key: DefaultKeys, def: String) -> String {
        return defaults.string(forKey: key.rawValue) ?? def
    }
    
    public static func bool(_ key: DefaultKeys) -> Bool {
        return defaults.bool(forKey: key.rawValue)
    }
    
    public static func int(_ key: DefaultKeys) -> Int {
        return defaults.integer(forKey: key.rawValue)
    }
    
    public static func double(_ key: DefaultKeys) -> Double {
        return defaults.double(forKey: key.rawValue)
    }
    
    public static func delete(_ key: DefaultKeys) {
        defaults.removeObject(forKey: key.rawValue)
    }
    
    // Utility methods
    public static func savedCredential() -> String {
        return string(.savedCredential, def: "")
    }
    
    public static func credentialType() -> CredentialType {
        return CredentialType(
            rawValue: string(
                .savedCredentialType,
                def: CredentialType.password.rawValue
            )
        ) ?? .password
    }
    
    public static func write(_ key: DefaultKeys, value: Any?) {
        defaults.set(value, forKey: key.rawValue)
    }
    
    public static func write(_ key: DefaultKeys, value: CredentialType) {
        defaults.set(value.rawValue, forKey: key.rawValue)
    }
}
