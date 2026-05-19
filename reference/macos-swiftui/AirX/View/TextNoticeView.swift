//
//  TextNotice.swift
//  SwiftUIPractice
//
//  Created by 刘世俊懿 on 2023-05-12.
//

import Foundation
import SwiftUI

struct TextNoticeView: View {
    @ObservedObject var viewModel = TextNoticeViewModel.shared
    @Binding var theme: Theme
    
    func onBlock() {
        
    }
    
    var body: some View {
        ZStack {
            VStack (spacing: 0){
                theme.gray.frame(height: 126)
                theme.blue.frame(height: 42)
            }
            .frame(width: 317, height: 168)
            
            VStack (spacing: 0) {
                VStack(alignment: .leading) {
                    Spacer().frame(height: 27)
                    
                    HStack {
                        Text(viewModel.receivedText)
                            .font(.system(size: 20, weight: .bold))
                            .foregroundColor(theme.textColor)
                        Spacer()
                    }
                    
                    Spacer()
                    
                    HStack {
                        Text("From \(viewModel.from.description)")
                            .font(.system(size: 13))
                            .foregroundColor(theme.textColor)
                        Spacer()
                    }
                    
                    Spacer().frame(height: 10)
                }
                .frame(height: 126)
                .padding(.leading)
                
                HStack {
                    Text("Copied.")
                        .foregroundColor(theme.buttonText)
                        .font(.system(size: 13, weight: .bold))
                        .focusable(false)
                        .padding(.leading, 17)
                        .padding(.vertical)

                    Spacer()

                    Button("BLOCK", action: onBlock)
                        .foregroundColor(theme.buttonText)
                        .font(.system(size: 13, weight: .bold))
                        .focusable(false)
                        .frame(width: 46, height: 16)
                        .background(theme.blue)
                        .buttonStyle(PlainButtonStyle())
                        .padding(.vertical)
                    
                    Spacer().frame(width: 15)
                    
                }
                .frame(height: 42)
            }
        }
        .frame(width: 317, height: 168)
    }
}

func truncatedText(_ message: String, maxLength: Int) -> String {
    if message.count <= maxLength {
        return message
    }
    let prefix = message.prefix(5)
    let suffix = message.suffix(4)
    return "\(prefix)...\(suffix)"
}

struct TextNotice_Previews: PreviewProvider {
    static var previews: some View {
        TextNoticeView(theme: .constant(LightMode()))
    }
}
