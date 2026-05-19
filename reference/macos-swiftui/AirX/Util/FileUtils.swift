//
//  FileUtil.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-07-14.
//

import Foundation
import AppKit

class FileUtils {
    public static func pathToNormalFormat(_ path: String) -> String {
        return path.replacingOccurrences(of: "\\\\", with: "/")
            .replacingOccurrences(of: "\\", with: "/")
            .replacingOccurrences(of: "//", with: "/")
    }
    

    /// /Users/miku/1.txt -> /Users/miku/
    public static func getPath(fullPath: String) -> String {
        let path = (pathToNormalFormat(fullPath) as NSString)
            .deletingLastPathComponent
        return path.removingPercentEncoding ?? path
    }
    
    /// /Users/miku/1.txt -> 1.txt
    /// C:\aaa\1.txt -> 1.txt
    public static func getFileName(fullPath: String) -> String {
        let path = (pathToNormalFormat(fullPath) as NSString)
            .lastPathComponent
        return path.removingPercentEncoding ?? path
    }
    
    private static var fileId: UInt8 = 0;
    public static func getNextFileId() -> UInt8 {
        if fileId == UInt8.max {
            fileId = 0
        }
        // OHHHHHHHHHHHHHHHHHHHH!
        defer { fileId += 1; }
        return fileId;
    }
    
    public static func getDownloadDirectoryUrl() -> URL {
        FileManager.default
            .urls(for: .downloadsDirectory, in: .userDomainMask)
            .first!
    }
    
    public static func showInFinder(fullPath: String) {
        NSWorkspace.shared.selectFile(fullPath, inFileViewerRootedAtPath: "")
    }
}
