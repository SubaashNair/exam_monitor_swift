//
//  AppState.swift
//  client_monitor
//
//  Created by Subashanan Nair on 22/03/2025.
//

import Foundation
import SwiftUI

enum Screen {
    case join
    case dashboard
}

class AppState: ObservableObject {
    @Published var currentScreen: Screen = .join
    @Published var studentName: String = ""
    @Published var studentID: String = ""
    @Published var joinCode: String = ""

    let client = Client()

    var isJoinFormValid: Bool {
        !studentName.trimmingCharacters(in: .whitespaces).isEmpty
            && studentID.trimmingCharacters(in: .whitespaces).count >= 3
            && joinCode.trimmingCharacters(in: .whitespaces).count == 4
    }

    func switchToJoin() {
        DispatchQueue.main.async {
            self.currentScreen = .join
        }
    }

    func switchToDashboard() {
        DispatchQueue.main.async {
            self.currentScreen = .dashboard
        }
    }

    func startClient() {
        guard isJoinFormValid else { return }
        // The class code alone identifies the room; the port comes from it.
        let code = joinCode.trimmingCharacters(in: .whitespaces).uppercased()

        client.start(
            studentName: studentName.trimmingCharacters(in: .whitespaces),
            studentID: studentID.trimmingCharacters(in: .whitespaces),
            joinCode: code,
            port: Int(portForCode(code))
        ) { [weak self] in
            DispatchQueue.main.async {
                self?.objectWillChange.send()
            }
        }

        switchToDashboard()
    }

    func stopClient() {
        client.stop()
        switchToJoin()
    }
}
