//
//  client_monitorApp.swift
//  client_monitor
//
//  Created by Subashanan Nair on 22/03/2025.
//

import SwiftUI

@main
struct client_monitorApp: App {
    @StateObject private var appState = AppState()
    
    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(appState)
                .frame(minWidth: 400, minHeight: 600)
        }
        .windowStyle(HiddenTitleBarWindowStyle())
        .commands {
            CommandGroup(replacing: .appInfo) {
                Button("About Exam Guard Client") {
                    NSApplication.shared.orderFrontStandardAboutPanel(
                        options: [
                            NSApplication.AboutPanelOptionKey.applicationName: "Exam Guard Client",
                            NSApplication.AboutPanelOptionKey.applicationVersion: "1.0",
                            NSApplication.AboutPanelOptionKey.credits: NSAttributedString(
                                string: "A secure screen sharing client for exam monitoring."
                            )
                        ]
                    )
                }
            }
        }
    }
}
