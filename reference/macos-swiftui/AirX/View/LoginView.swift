//
//  login.swift
//  SwiftUIPractice
//
//  Created by 刘世俊懿 on 2023-05-24.
//

import Foundation
import SwiftUI
import GoogleSignIn
import GoogleSignInSwift

struct LoginView: View {
    @Environment(\.presentationMode) var presentationMode
    
    @State private var uid: String
        = Defaults.string(.savedUsername, def: "")
    
    @State private var password: String
        = Defaults.savedCredential()
    
    @State private var shouldRememberPassword: Bool
        = Defaults.bool(.shouldRememberPassword)
    
    @State private var shouldShowContentView: Bool = false
    @State private var isLoggingIn: Bool = false
    @State private var shouldShowAlert: Bool = false
    @State private var errorMessage: String = ""
    
    @Binding var isSignedInRef: Bool
    
    
    func onSignInClicked() {
        Task {
            isLoggingIn = true
            do {
                try AnyDropCloud.login(uidOrEmail: uid, password: password) { response in
                    guard response.success else {
                        errorMessage = response.message
                        shouldShowAlert = true
                        return
                    }
                    
                    // Success
                    Defaults.write(.savedCredential, value: response.token)
                    Defaults.write(.savedCredentialType, value: .anydropToken)
                    Defaults.write(.loggedInUid, value: uid)
                    GlobalState.shared.isSignedIn = true

                    if shouldRememberPassword {
                        Defaults.write(.savedUsername, value: uid)
                    }
                    
                    if !WebSocketService.shared.initialize() {
                        print("Failed to register with backend.")
                    }
                    presentationMode.wrappedValue.dismiss()
                }
            }
            catch {
                errorMessage = error.localizedDescription
                shouldShowAlert = true
                isLoggingIn = false
                return
            }
            isLoggingIn = false
        }
    }
    
    func onSignUpClicked() {
        if let url = URL(string: "https://anydrop-cloud.eggtartc.com/sign-up") {
            NSWorkspace.shared.open(url)
        }
    }
    
    func onRememberMeChanged(newValue: Bool) {
        UserDefaults.standard.set(newValue, forKey: "ShouldRememberPassword")
        
        if !newValue {
            UserDefaults.standard.removeObject(forKey: "SavedPassword")
            password = ""
        }
    }
    
    func onOpenUrl(url: URL) {
        GIDSignIn.sharedInstance.handle(url)
    }
    
    func onGoogleSignInClicked() {
        guard let presentingWindow = NSApplication.shared.windows.first else {
            return
        }
        GIDSignIn.sharedInstance.signIn(withPresenting: presentingWindow) { signInResult, error in
            guard let result = signInResult else {
                // Inspect error
                errorMessage = error?.localizedDescription ?? "Unknown error"
                shouldShowAlert = true
                return
            }
            // If sign in succeeded, display the app's main content View.
            // TODO: continue google signin
            print(result)
        }
    }
    
    var body: some View {
        VStack {
            HStack {
                VStack(alignment: .trailing) {
                    Text("Account").frame(height: 20)
                    Text("Password").frame(height: 20)
                }.frame(width: 80, alignment: .trailing)
                
                VStack {
                    TextField("Enter UID/Email", text: $uid)
                        .frame(height: 20)
                        .onSubmit(onSignInClicked)
                    
                    SecureField("Enter Password", text: $password)
                        .frame(height: 20)
                        .onSubmit(onSignInClicked)
                }
            }
            
            Toggle("Remember Me", isOn: $shouldRememberPassword)
                .disabled(isLoggingIn)
                .onChange(
                    of: shouldRememberPassword,
                    perform: onRememberMeChanged
                )
            
            HStack {
                Button("Sign In", action: onSignInClicked).disabled(isLoggingIn)
                Button("Sign Up", action: onSignUpClicked).disabled(isLoggingIn)
            }

            //忘记密码
            HStack {
                Link("Forgot your password?", destination: URL(string: "http://shijunyi-cv.com/shijunyi/shijunyi.html#home")!)
            }
            
            Divider()
            
            GoogleSignInButton(
                viewModel: GoogleSignInButtonViewModel(
                    scheme: .light, style: .wide, state: isLoggingIn ? .disabled : .normal),
                action: onGoogleSignInClicked
            )
        }
        .frame(width: 300, height: 200)
        .padding()
        .onOpenURL(perform: onOpenUrl)
        .alert(errorMessage, isPresented: $shouldShowAlert) {
            Button("OK", role: .cancel, action: {})
        }
        /*
         .sheet(isPresented: $shouldShowContentView) {
         ContentView()
         }
         */
    }
}

struct LoginView_Previews: PreviewProvider {
    static var previews: some View {
        LoginView(isSignedInRef: .constant(false))
    }
}

