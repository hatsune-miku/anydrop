//
//  AboutView.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-06-14.
//

import Foundation
import SwiftUI

struct AboutView: View {
    var body: some View {
        VStack {
            VStack(alignment: .leading) {
                Text("AnyDrop - Text/File Sync Tool")
                    .font(.system(size: 18))
                    .bold()
                Text("macOS Frontend")
            }
            
            Divider()
            
            Text("- Components -")
            Grid(alignment: .leading) {
                GridRow(alignment: .top) {
                    Text(Bundle.main.appName).bold()
                    VStack(alignment: .leading) {
                        Text(verbatim: "Version \(Bundle.main.appVersionLong)")
                        Text(verbatim: "Build \(Bundle.main.appBuild)")
                    }
                }
                GridRow(alignment: .top) {
                    Text("libanydrop").bold()
                    VStack(alignment: .leading) {
                        Text(verbatim: "Version \(anydrop_version())")
                        Text(verbatim: "\(AnyDropService.readVersionString())")
                    }
                }
            }

            Divider()
            
            Text("Memorial University")
            Text(Bundle.main.copyright)
        }
        .frame(width: 305, height: 245)
    }
}

struct AboutView_Previews: PreviewProvider {
    static var previews: some View {
        AboutView()
    }
}

