//
//  ExamGuardServerApp.swift
//  server_monitor
//
//  Created by Subashanan Nair on 23/03/2025.
//

import SwiftUI

@main
struct ExamGuardServerApp: App {
    @StateObject private var serverManager = Server()
    
    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(serverManager)
                .frame(minWidth: 800, minHeight: 600)
        }
        .windowStyle(HiddenTitleBarWindowStyle())
        .commands {
            CommandGroup(replacing: .appInfo) {
                Button("About Exam Guard Server") {
                    NSApplication.shared.orderFrontStandardAboutPanel(
                        options: [
                            NSApplication.AboutPanelOptionKey.applicationName: "Exam Guard Server",
                            NSApplication.AboutPanelOptionKey.applicationVersion: "1.0",
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
