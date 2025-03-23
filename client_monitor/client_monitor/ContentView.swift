//
//  ContentView.swift
//  client_monitor
//
//  Created by Subashanan Nair on 22/03/2025.
//

import SwiftUI

struct ContentView: View {
    @EnvironmentObject var appState: AppState
    
    var body: some View {
        ZStack {
            switch appState.currentScreen {
            case .join:
                JoinView()
            case .dashboard:
                DashboardView()
            }
        }
        .frame(minWidth: 400, minHeight: 600)
    }
}

struct JoinView: View {
    @EnvironmentObject var appState: AppState
    
    var body: some View {
        VStack(spacing: 20) {
            Text("Exam Guard Client")
                .font(.title)
                .padding(.top, 40)
            
            VStack(alignment: .leading, spacing: 8) {
                Text("Your Name")
                    .font(.headline)
                
                TextField("Enter your name", text: $appState.studentName)
                    .textFieldStyle(RoundedBorderTextFieldStyle())
                    .frame(maxWidth: 300)
            }
            .padding(.top, 30)
            
            VStack(alignment: .leading, spacing: 8) {
                Text("Room Number")
                    .font(.headline)
                
                TextField("Enter room number", text: $appState.roomNumber)
                    .textFieldStyle(RoundedBorderTextFieldStyle())
                    .frame(maxWidth: 300)
            }
            .padding(.top, 10)
            
            Button(action: {
                appState.startClient()
            }) {
                Text("Start")
                    .font(.headline)
                    .foregroundColor(.white)
                    .padding()
                    .frame(width: 200)
                    .background(Color.blue)
                    .cornerRadius(10)
            }
            .buttonStyle(.plain)
            .padding(.top, 30)
            .disabled(appState.studentName.isEmpty || appState.roomNumber.isEmpty)
            
            Spacer()
        }
        .padding()
    }
}

struct DashboardView: View {
    @EnvironmentObject var appState: AppState
    
    var body: some View {
        VStack {
            if appState.client.isConnected {
                Text("Screen Sharing Started")
                    .font(.title)
                    .padding()
            } else {
                Text("Looking for the server")
                    .font(.title)
                    .padding()
                
                Button(action: {
                    appState.stopClient()
                }) {
                    Text("Stop")
                        .font(.headline)
                        .foregroundColor(.white)
                        .padding()
                        .frame(width: 200)
                        .background(Color.red)
                        .cornerRadius(10)
                }
                .buttonStyle(.plain)
                .padding(.top, 20)
            }
            
            Spacer()
        }
        .padding()
    }
}

// Preview for SwiftUI canvas
struct ContentView_Previews: PreviewProvider {
    static var previews: some View {
        let appState = AppState()
        return ContentView().environmentObject(appState)
    }
}
