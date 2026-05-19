//
//  HomeView.swift
//  server_monitor
//
//  Created by Subashanan Nair on 23/03/2025.
//

import SwiftUI

struct HomeView: View {
    @EnvironmentObject var server: Server
    @State private var examName: String = ""
    @State private var courseName: String = ""
    @State private var roomNumber: String = ""
    @State private var showError = false
    @State private var errorMessage = ""

    var body: some View {
        VStack(spacing: 24) {
            VStack(spacing: 6) {
                Text("Exam Guard Server")
                    .font(.title)
                    .fontWeight(.semibold)
                Text("Create a monitoring room")
                    .font(.subheadline)
                    .foregroundColor(.secondary)
            }
            .padding(.top, 32)

            VStack(spacing: 16) {
                ServerFieldRow(label: "Exam Name", placeholder: "e.g. Final Examination", text: $examName)
                ServerFieldRow(label: "Course / Module", placeholder: "e.g. CS3010 Distributed Systems", text: $courseName)
                ServerFieldRow(label: "Room Number", placeholder: "4-digit room code", text: $roomNumber)
                    .onChange(of: roomNumber) { _, newValue in
                        let filtered = String(newValue.filter { $0.isNumber }.prefix(4))
                        if filtered != newValue { roomNumber = filtered }
                    }
            }
            .frame(maxWidth: 380)

            Button(action: startRoom) {
                Text("Create Room")
                    .font(.headline)
                    .foregroundColor(.white)
                    .frame(maxWidth: 380, minHeight: 44)
                    .background(isInputValid ? Color.accentColor : Color.gray.opacity(0.5))
                    .cornerRadius(10)
            }
            .buttonStyle(.plain)
            .disabled(!isInputValid)

            if showError {
                Text(errorMessage)
                    .foregroundColor(.red)
                    .font(.subheadline)
            }

            Spacer()
        }
        .padding(.horizontal, 32)
        .padding(.bottom, 24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var isInputValid: Bool {
        !examName.trimmingCharacters(in: .whitespaces).isEmpty
            && !courseName.trimmingCharacters(in: .whitespaces).isEmpty
            && roomNumber.count == 4
    }

    private func startRoom() {
        guard let roomNum = Int(roomNumber), roomNum > 0 else {
            errorMessage = "Please enter a valid 4-digit room number"
            showError = true
            return
        }

        server.examName = examName.trimmingCharacters(in: .whitespaces)
        server.courseName = courseName.trimmingCharacters(in: .whitespaces)
        server.roomNumber = roomNumber
        server.start(port: roomNum)
    }
}

private struct ServerFieldRow: View {
    let label: String
    let placeholder: String
    @Binding var text: String

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label)
                .font(.subheadline)
                .fontWeight(.medium)
                .foregroundColor(.secondary)
            TextField(placeholder, text: $text)
                .textFieldStyle(RoundedBorderTextFieldStyle())
        }
    }
}

struct HomeView_Previews: PreviewProvider {
    static var previews: some View {
        HomeView()
            .environmentObject(Server())
    }
}
