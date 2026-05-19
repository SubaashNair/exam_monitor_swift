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
        .frame(minWidth: 460, minHeight: 620)
    }
}

struct JoinView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        VStack(spacing: 24) {
            VStack(spacing: 6) {
                Text("Exam Guard Client")
                    .font(.title)
                    .fontWeight(.semibold)

                Text("Join a monitoring room to start sharing your screen.")
                    .font(.subheadline)
                    .foregroundColor(.secondary)
                    .multilineTextAlignment(.center)
            }
            .padding(.top, 32)

            VStack(spacing: 16) {
                FieldRow(label: "Full Name", placeholder: "e.g. Ada Lovelace", text: $appState.studentName)

                FieldRow(label: "Student ID", placeholder: "e.g. STU-2025-001", text: $appState.studentID)

                FieldRow(label: "Room Number", placeholder: "4-digit room code", text: $appState.roomNumber)
                    .onChange(of: appState.roomNumber) { _, newValue in
                        let filtered = String(newValue.filter { $0.isNumber }.prefix(4))
                        if filtered != newValue { appState.roomNumber = filtered }
                    }
            }
            .frame(maxWidth: 360)

            Button(action: appState.startClient) {
                Text("Start Sharing")
                    .font(.headline)
                    .foregroundColor(.white)
                    .frame(maxWidth: 360, minHeight: 44)
                    .background(appState.isJoinFormValid ? Color.accentColor : Color.gray.opacity(0.5))
                    .cornerRadius(10)
            }
            .buttonStyle(.plain)
            .disabled(!appState.isJoinFormValid)

            Spacer()
        }
        .padding(.horizontal, 32)
        .padding(.bottom, 24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct FieldRow: View {
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

struct DashboardView: View {
    @EnvironmentObject var appState: AppState

    private var statusText: String {
        appState.client.isConnected ? "Screen sharing is active" : "Looking for the server…"
    }

    private var statusColor: Color {
        appState.client.isConnected ? .green : .orange
    }

    var body: some View {
        VStack(spacing: 24) {
            VStack(spacing: 4) {
                Text("Exam Guard Client")
                    .font(.title2)
                    .fontWeight(.semibold)
                Text(statusText)
                    .font(.subheadline)
                    .foregroundColor(.secondary)
            }
            .padding(.top, 32)

            VStack(spacing: 16) {
                HStack(spacing: 12) {
                    Circle()
                        .fill(statusColor)
                        .frame(width: 10, height: 10)
                        .shadow(color: statusColor.opacity(0.6), radius: 4)
                    Text(appState.client.isConnected ? "Connected" : "Not connected")
                        .font(.headline)
                    Spacer()
                }

                Divider()

                InfoRow(label: "Student name", value: appState.studentName)
                InfoRow(label: "Student ID", value: appState.studentID)
                InfoRow(label: "Room number", value: appState.roomNumber)
            }
            .padding(20)
            .background(Color(NSColor.controlBackgroundColor))
            .cornerRadius(12)
            .frame(maxWidth: 380)

            Button(action: appState.stopClient) {
                Text("Stop Sharing")
                    .font(.headline)
                    .foregroundColor(.white)
                    .frame(maxWidth: 380, minHeight: 44)
                    .background(Color.red)
                    .cornerRadius(10)
            }
            .buttonStyle(.plain)

            Spacer()
        }
        .padding(.horizontal, 32)
        .padding(.bottom, 24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct InfoRow: View {
    let label: String
    let value: String

    var body: some View {
        HStack {
            Text(label)
                .font(.subheadline)
                .foregroundColor(.secondary)
            Spacer()
            Text(value.isEmpty ? "—" : value)
                .font(.subheadline)
                .fontWeight(.medium)
        }
    }
}

struct ContentView_Previews: PreviewProvider {
    static var previews: some View {
        ContentView().environmentObject(AppState())
    }
}
