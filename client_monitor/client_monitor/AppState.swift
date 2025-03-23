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
    @Published var roomNumber: String = ""
    
    let client = Client()
    
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
        guard let roomNum = Int(roomNumber), !studentName.isEmpty else {
            return
        }
        
        client.start(studentName: studentName, port: roomNum) { [weak self] in
            // This will be called when UI update is needed
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
