//
//  ContentView.swift
//  server_monitor
//
//  Created by Subashanan Nair on 23/03/2025.
//

//import SwiftUI
//
//struct ContentView: View {
//    @EnvironmentObject var server: Server
//    
//    var body: some View {
//        ZStack {
//            if server.isRunning {
//                DashboardView()
//            } else {
//                HomeView()
//            }
//        }
//    }
//}
//
//struct ContentView_Previews: PreviewProvider {
//    static var previews: some View {
//        ContentView()
//            .environmentObject(Server())
//    }
//}
import SwiftUI

struct ContentView: View {
    @EnvironmentObject var server: Server
    
    var body: some View {
        ZStack {
            if server.isRunning {
                DashboardView()
            } else {
                HomeView()
            }
        }
    }
}

struct ContentView_Previews: PreviewProvider {
    static var previews: some View {
        ContentView()
            .environmentObject(Server())
    }
}
