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
    @Published var roomNumber: String = ""

    let client = Client()

    var isJoinFormValid: Bool {
        !studentName.trimmingCharacters(in: .whitespaces).isEmpty
            && studentID.trimmingCharacters(in: .whitespaces).count >= 3
            && Int(roomNumber).map { $0 > 0 } == true
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
        guard isJoinFormValid, let roomNum = Int(roomNumber) else { return }

        client.start(
            studentName: studentName.trimmingCharacters(in: .whitespaces),
            studentID: studentID.trimmingCharacters(in: .whitespaces),
            port: roomNum
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
