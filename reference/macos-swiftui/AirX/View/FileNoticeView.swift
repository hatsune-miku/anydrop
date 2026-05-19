//
//  FileNotice.swift
//  SwiftUIPractice
//
//  Created by 刘世俊懿 on 2023-05-12.
//

import Foundation
import SwiftUI
import Combine

struct FileNoticeView: View {
    @Environment(\.presentationMode) var presentationMode

    @State private var theme: Theme = LightMode()
    @ObservedObject var receivingFile: ReceiveFile
    
    func onBlock() {
        guard UIUtils.alertBox(
            title: "Stop",
            message: "Are you sure to block \(receivingFile.from.description)?",
            primaryButtonText: "Block",
            secondaryButtonText: "Don't Block"
        ) == .alertFirstButtonReturn else {
            return
        }
        
        AccountUtils.blockUser(peer: receivingFile.from)
    }
    
    func onStop() {
        guard UIUtils.alertBox(
            title: "Stop",
            message: "Are you sure to stop receiving the file?",
            primaryButtonText: "Stop",
            secondaryButtonText: "Don't Stop"
        ) == .alertFirstButtonReturn else {
            return
        }

        receivingFile.status = .cancelledByReceiver
        presentationMode.wrappedValue.dismiss()
    }
    
    func onOpenFolder() {
        FileUtils.showInFinder(
            fullPath: receivingFile.localSaveFullPath.path(percentEncoded: false))
    }
    
    var body: some View {
        let progressPercent = Double(receivingFile.progress) / Double(receivingFile.totalSize)
        
        ZStack {
            VStack (spacing: 0) {
                theme.gray.frame(height: 126)
                theme.blue.frame(height: 42)
            }
            .frame(width: 317, height: 168)
            
            Spacer()
            
            VStack (spacing: 0) {
                VStack(alignment: .leading) {
                    Spacer().frame(width: 27)
                    
                    HStack {
                        Text(
                            truncatedFilename(
                                FileUtils.getFileName(
                                    fullPath: receivingFile.localSaveFullPath.path(percentEncoded: false)),
                                maxLength: 10)
                        )
                            .font(.system(size: 20, weight: .bold))
                            .foregroundColor(theme.textColor)
                    }
                    
                    HStack {
                        Text(receivingFile.sizeRepresentation)
                            .font(.system(size: 16))
                            .foregroundColor(theme.textColor)
                        Spacer() // 将文本推向左侧
                    }
                    
                    Spacer().frame(width: 21)
                    
                    HStack {
                        Text("From \(receivingFile.from.description)")
                            .font(.system(size: 13))
                            .foregroundColor(theme.textColor)
                        Spacer() // 将文本推向左侧
                    }
                    
                    Spacer().frame(height: 10)
                }
                .frame(height: 126)
                .padding(.leading) // 在左侧添加一些间距
                
                // 下半部分
                HStack {
                    // 进度条
                    ProgressView(value: progressPercent)
                        .frame(width: 75, height: 7)
                        .progressViewStyle(ColoredProgressViewStyle(color: theme.progressColor))
                        .background(theme.progressTrack) // 设置进度条未完成部分的背景色
                        .padding(.leading, 17)
                        .padding(.vertical)
                    
                    Text(String(format: "%.2f%%", progressPercent * 100))
                        .foregroundColor(theme.textColor)
                        .font(.system(size: 9, weight: .bold))
                    
                    Spacer()
                    
                    if progressPercent == 1 {
                        Button("OPEN FOLDER", action: onOpenFolder)
                            .buttonStyle(.plain)
                            .font(.system(size: 13, weight: .bold))
                            .foregroundColor(theme.buttonText)
                            .padding(.vertical)
                            .focusable(false)
                    }
                    else {
                        Button("STOP", action: onStop)
                            .buttonStyle(.plain)
                            .font(.system(size: 13, weight: .bold))
                            .foregroundColor(theme.buttonText)
                            .padding(.vertical)
                            .focusable(false)
                    }
                    
                    Spacer().frame(width: 26)
                    
                    Button("BLOCK", action: onBlock)
                        .buttonStyle(.plain)
                        .font(.system(size: 13, weight: .bold))
                        .foregroundColor(theme.buttonText)
                        .padding(.vertical)
                        .focusable(false)

                    Spacer().frame(width: 15)
                }
                .frame(height: 42)
            } // VStack
        }.frame(width: 317, height: 168) // ZStack
    } // some View
}

struct ColoredProgressViewStyle: ProgressViewStyle {
    var color: Color
    
    func makeBody(configuration: Configuration) -> some View {
        ProgressView(configuration)
            .padding(.vertical)
            .progressViewStyle(LinearProgressViewStyle(tint: color))
    }
}

func truncatedFilename(_ filename: String, maxLength: Int) -> String {
    if filename.count <= maxLength {
        return filename
    }
    
    let suffix = filename.split(separator: ".").last ?? ""
    let basename = filename.prefix(while: { $0 != "." })
    
    if basename.count > maxLength {
        let startIndex = filename.startIndex
        let truncatedIndex = filename.index(startIndex, offsetBy: maxLength - suffix.count - 1)
        return "\(filename[startIndex...truncatedIndex])….\(suffix)"
    }
    
    let truncatedIndex = basename.index(basename.startIndex, offsetBy: maxLength - suffix.count - 1)
    return "\(basename[basename.startIndex...truncatedIndex])….\(suffix)"
}

struct FileNotice_Previews: PreviewProvider {
    static var previews: some View {
        FileNoticeView(
            receivingFile: GlobalState.shared.receiveFiles[255]!)
    }
}
