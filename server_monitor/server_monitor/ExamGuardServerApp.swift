//
//  ExamGuardServerApp.swift
//  server_monitor
//
//  Created by Subashanan Nair on 23/03/2025.
//

import SwiftUI

struct AppVersionBadge: View {
    static let current = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev"

    var body: some View {
        Text("v\(Self.current)")
            .font(.caption2)
            .foregroundColor(.secondary)
            .padding(6)
            .allowsHitTesting(false)
    }
}

@main
struct ExamGuardServerApp: App {
    @StateObject private var serverManager = Server()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(serverManager)
                .frame(minWidth: 800, minHeight: 600)
                .overlay(alignment: .bottomTrailing) { AppVersionBadge() }
        }
        .windowStyle(HiddenTitleBarWindowStyle())
        .commands {
            CommandGroup(replacing: .appInfo) {
                Button("About Exam Guard Server") {
                    NSApplication.shared.orderFrontStandardAboutPanel(
                        options: [
                            NSApplication.AboutPanelOptionKey.applicationName: "Exam Guard Server",
                            NSApplication.AboutPanelOptionKey.applicationVersion: AppVersionBadge.current,
                            NSApplication.AboutPanelOptionKey.credits: NSAttributedString(
                                string: "A secure screen monitoring server for exam proctoring."
                            )
                        ]
                    )
                }
            }
        }
    }
}
