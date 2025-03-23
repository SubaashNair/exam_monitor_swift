//
//  HomeView.swift
//  server_monitor
//
//  Created by Subashanan Nair on 23/03/2025.
//

import SwiftUI

struct HomeView: View {
    @EnvironmentObject var server: Server
    @State private var roomNumber: String = ""
    @State private var showError = false
    @State private var errorMessage = ""
    
    var body: some View {
        VStack(spacing: 30) {
            Text("Exam Guard Server")
                .font(.largeTitle)
                .fontWeight(.bold)
                .padding(.top, 50)
            
            Text("Create a monitoring room")
                .font(.title2)
            
            VStack(alignment: .leading, spacing: 10) {
                Text("Room Number")
                    .font(.headline)
                
                TextField("Enter 4-digit room number", text: $roomNumber)
                    .frame(maxWidth: 300)
                    .textFieldStyle(RoundedBorderTextFieldStyle())
//                    .keyboardType(.numberPad)
                    .onChange(of: roomNumber) {oldValue, newValue in
                        // Limit to 4 digits and numeric only
                        if newValue.count > 4 {
                            roomNumber = String(newValue.prefix(4))
                        }
                        roomNumber = newValue.filter { $0.isNumber }
                    }
            }
            .padding(.bottom, 20)
            
            Button(action: startRoom) {
                Text("Create Room")
                    .fontWeight(.semibold)
                    .foregroundColor(.white)
                    .padding()
                    .frame(width: 200)
                    .background(isInputValid ? Color.blue : Color.gray)
                    .cornerRadius(10)
            }
            .buttonStyle(.plain)
            .disabled(!isInputValid)
            
            Spacer()
            
            if showError {
                Text(errorMessage)
                    .foregroundColor(.red)
                    .padding()
            }
        }
        .padding()
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
    
    private var isInputValid: Bool {
        return roomNumber.count == 4
    }
    
    private func startRoom() {
        guard let roomNum = Int(roomNumber), roomNum > 0 else {
            errorMessage = "Please enter a valid room number"
            showError = true
            return
        }
        
        // Start the server with the specified room number
        server.start(port: roomNum)
    }
}

struct HomeView_Previews: PreviewProvider {
    static var previews: some View {
        HomeView()
            .environmentObject(Server())
    }
}
