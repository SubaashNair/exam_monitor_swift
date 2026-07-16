//
//  DashboardView.swift
//  server_monitor
//
//  Created by Subashanan Nair on 23/03/2025.
//

//import SwiftUI
//
//struct DashboardView: View {
//    @EnvironmentObject var server: Server
//    @State private var gridColumns = [GridItem]()
//    @State private var selectedStudent: Student? = nil
//    
//    init() {
//        // Default to 3 columns
//        updateGridColumns(count: 3)
//    }
//    
//    var body: some View {
//        VStack(spacing: 0) {
//            // Header
//            HStack {
//                Text("Connected Students: \(server.students.count)")
//                    .font(.headline)
//                    .padding()
//                
//                Spacer()
//                
//                // Column adjustment controls
//                HStack(spacing: 15) {
//                    Button(action: {
//                        updateGridColumns(count: max(gridColumns.count - 1, 1))
//                    }) {
//                        Image(systemName: "minus.circle")
//                            .font(.title3)
//                    }
//                    .buttonStyle(PlainButtonStyle())
//                    .disabled(gridColumns.count <= 1)
//                    
//                    Text("\(gridColumns.count) columns")
//                        .font(.subheadline)
//                    
//                    Button(action: {
//                        updateGridColumns(count: min(gridColumns.count + 1, 6))
//                    }) {
//                        Image(systemName: "plus.circle")
//                            .font(.title3)
//                    }
//                    .buttonStyle(PlainButtonStyle())
//                    .disabled(gridColumns.count >= 6)
//                }
//                .padding(.horizontal)
//                
//                Button(action: {
//                    server.stop()
//                }) {
//                    Text("Stop Monitoring")
//                        .foregroundColor(.white)
//                        .padding(.horizontal, 20)
//                        .padding(.vertical, 8)
//                        .background(Color.red)
//                        .cornerRadius(8)
//                }
//                .padding()
//            }
//            .background(Color(NSColor.windowBackgroundColor))
//            .overlay(
//                Rectangle()
//                    .frame(height: 1)
//                    .foregroundColor(Color(NSColor.separatorColor)),
//                alignment: .bottom
//            )
//            
//            // Student grid
//            if server.students.isEmpty {
//                VStack {
//                    Spacer()
//                    Text("Waiting for students to connect...")
//                        .font(.title2)
//                        .foregroundColor(.secondary)
//                    Spacer()
//                }
//            } else {
//                ScrollView {
//                    LazyVGrid(columns: gridColumns, spacing: 16) {
//                        ForEach(server.students) { student in
//                            StudentThumbnail(student: student, isSelected: selectedStudent?.id == student.id)
//                                .onTapGesture {
//                                    selectedStudent = student
//                                }
//                        }
//                    }
//                    .padding()
//                }
//            }
//        }
//        .sheet(item: $selectedStudent) { student in
//            StudentDetailView(student: student)
//                .frame(width: 800, height: 600)
//        }
//    }
//    
//    private func updateGridColumns(count: Int) {
//        gridColumns = Array(repeating: GridItem(.flexible(), spacing: 16), count: count)
//    }
//}
//
//struct StudentThumbnail: View {
//    let student: Student
//    let isSelected: Bool
//    
//    var body: some View {
//        VStack {
//            if let image = student.image {
//                Image(nsImage: image)
//                    .resizable()
//                    .aspectRatio(contentMode: .fill)
//                    .frame(minHeight: 160)
//                    .cornerRadius(4)
//                    .overlay(
//                        RoundedRectangle(cornerRadius: 4)
//                            .stroke(isSelected ? Color.blue : Color.clear, lineWidth: 3)
//                    )
//            } else {
//                Rectangle()
//                    .fill(Color.secondary.opacity(0.2))
//                    .aspectRatio(16/9, contentMode: .fit)
//                    .overlay(
//                        Image(systemName: "display")
//                            .font(.system(size: 30))
//                            .foregroundColor(.secondary)
//                    )
//                    .cornerRadius(4)
//            }
//            
//            HStack {
//                Text(student.name)
//                    .font(.headline)
//                    .lineLimit(1)
//                
//                Spacer()
//                
//                // Show when the image was last updated
//                Text(timeAgoString(from: student.lastUpdate))
//                    .font(.caption)
//                    .foregroundColor(.secondary)
//            }
//        }
//        .padding(8)
//        .background(Color(NSColor.controlBackgroundColor))
//        .cornerRadius(8)
//    }
//    
//    private func timeAgoString(from date: Date) -> String {
//        let seconds = Int(Date().timeIntervalSince(date))
//        
//        if seconds < 15 {
//            return "Just now"
//        } else if seconds < 60 {
//            return "\(seconds)s ago"
//        } else if seconds < 3600 {
//            return "\(seconds / 60)m ago"
//        } else {
//            return "\(seconds / 3600)h ago"
//        }
//    }
//}
//
//struct StudentDetailView: View {
//    let student: Student
//    
//    var body: some View {
//        VStack(spacing: 20) {
//            Text(student.name)
//                .font(.largeTitle)
//                .padding(.top)
//            
//            if let image = student.image {
//                Image(nsImage: image)
//                    .resizable()
//                    .aspectRatio(contentMode: .fit)
//                    .frame(maxWidth: .infinity, maxHeight: .infinity)
//            } else {
//                Rectangle()
//                    .fill(Color.secondary.opacity(0.2))
//                    .aspectRatio(16/9, contentMode: .fit)
//                    .overlay(
//                        Image(systemName: "display")
//                            .font(.system(size: 50))
//                            .foregroundColor(.secondary)
//                    )
//            }
//            
//            HStack {
//                Button(action: {}) {
//                    Label("Send Message", systemImage: "message")
//                        .padding(.horizontal)
//                }
//                .buttonStyle(BorderedButtonStyle())
//                
//                Button(action: {}) {
//                    Label("Flag Student", systemImage: "flag")
//                        .padding(.horizontal)
//                }
//                .buttonStyle(BorderedButtonStyle())
//                
//                Button(action: {}) {
//                    Label("Disconnect", systemImage: "x.circle")
//                        .padding(.horizontal)
//                }
//                .buttonStyle(BorderedButtonStyle())
//            }
//            .padding(.bottom)
//        }
//        .frame(maxWidth: .infinity, maxHeight: .infinity)
//    }
//}


//import SwiftUI
//
//struct DashboardView: View {
//    @EnvironmentObject var server: Server
//    @State private var gridColumns = [GridItem]()
//    @State private var selectedStudent: Student? = nil
//    
//    init() {
//        // Default to 3 columns
//        updateGridColumns(count: 3)
//    }
//    
//    var body: some View {
//        VStack(spacing: 0) {
//            // Header
//            HStack {
//                Text("Connected Students: \(server.students.count)")
//                    .font(.headline)
//                    .padding()
//                
//                Spacer()
//                
//                // Column adjustment controls
//                HStack(spacing: 15) {
//                    Button(action: {
//                        updateGridColumns(count: max(gridColumns.count - 1, 1))
//                    }) {
//                        Image(systemName: "minus.circle")
//                            .font(.title3)
//                    }
//                    .buttonStyle(PlainButtonStyle())
//                    .disabled(gridColumns.count <= 1)
//                    
//                    Text("\(gridColumns.count) columns")
//                        .font(.subheadline)
//                    
//                    Button(action: {
//                        updateGridColumns(count: min(gridColumns.count + 1, 6))
//                    }) {
//                        Image(systemName: "plus.circle")
//                            .font(.title3)
//                    }
//                    .buttonStyle(PlainButtonStyle())
//                    .disabled(gridColumns.count >= 6)
//                }
//                .padding(.horizontal)
//                
//                Button(action: {
//                    server.stop()
//                }) {
//                    Text("Stop Monitoring")
//                        .foregroundColor(.white)
//                        .padding(.horizontal, 20)
//                        .padding(.vertical, 8)
//                        .background(Color.red)
//                        .cornerRadius(8)
//                }
//                .padding()
//            }
//            .background(Color(NSColor.windowBackgroundColor))
//            .overlay(
//                Rectangle()
//                    .frame(height: 1)
//                    .foregroundColor(Color(NSColor.separatorColor)),
//                alignment: .bottom
//            )
//            
//            // Student grid
//            if server.students.isEmpty {
//                VStack {
//                    Spacer()
//                    Text("Waiting for students to connect...")
//                        .font(.title2)
//                        .foregroundColor(.secondary)
//                    Spacer()
//                }
//            } else {
//                ScrollView {
//                    LazyVGrid(columns: gridColumns, spacing: 16) {
//                        ForEach(server.students) { student in
//                            StudentThumbnail(student: student, isSelected: selectedStudent?.id == student.id)
//                                .onTapGesture {
//                                    selectedStudent = student
//                                }
//                        }
//                    }
//                    .padding()
//                }
//            }
//        }
//        .sheet(item: $selectedStudent) { student in
//            StudentDetailView(student: student)
//                .frame(width: 800, height: 600)
//        }
//        // Add these debugging modifiers
//        .onAppear {
//            print("SERVER UI: Dashboard appeared, \(server.students.count) students")
//        }
//        .onChange(of: server.students.count) { newCount in
//            print("SERVER UI: Student count changed to: \(newCount)")
//        }
//    }
//    
//    private func updateGridColumns(count: Int) {
//        gridColumns = Array(repeating: GridItem(.flexible(), spacing: 16), count: count)
//    }
//}
//
//struct StudentThumbnail: View {
//    let student: Student
//    let isSelected: Bool
//    
//    var body: some View {
//        VStack {
//            if let image = student.image {
//                Image(nsImage: image)
//                    .resizable()
//                    .aspectRatio(contentMode: .fill)
//                    .frame(minHeight: 160)
//                    .cornerRadius(4)
//                    .overlay(
//                        RoundedRectangle(cornerRadius: 4)
//                            .stroke(isSelected ? Color.blue : Color.clear, lineWidth: 3)
//                    )
//            } else {
//                Rectangle()
//                    .fill(Color.secondary.opacity(0.2))
//                    .aspectRatio(16/9, contentMode: .fit)
//                    .overlay(
//                        Image(systemName: "display")
//                            .font(.system(size: 30))
//                            .foregroundColor(.secondary)
//                    )
//                    .cornerRadius(4)
//            }
//            
//            HStack {
//                Text(student.name)
//                    .font(.headline)
//                    .lineLimit(1)
//                
//                Spacer()
//                
//                // Show when the image was last updated
//                Text(timeAgoString(from: student.lastUpdate))
//                    .font(.caption)
//                    .foregroundColor(.secondary)
//            }
//        }
//        .padding(8)
//        .background(Color(NSColor.controlBackgroundColor))
//        .cornerRadius(8)
//        // Add this debugging modifier
//        .onAppear {
//            print("Rendering student: \(student.name), has image: \(student.image != nil), ID: \(student.id)")
//        }
//    }
//    
//    private func timeAgoString(from date: Date) -> String {
//        let seconds = Int(Date().timeIntervalSince(date))
//        
//        if seconds < 15 {
//            return "Just now"
//        } else if seconds < 60 {
//            return "\(seconds)s ago"
//        } else if seconds < 3600 {
//            return "\(seconds / 60)m ago"
//        } else {
//            return "\(seconds / 3600)h ago"
//        }
//    }
//}
//
//struct StudentDetailView: View {
//    let student: Student
//    
//    var body: some View {
//        VStack(spacing: 20) {
//            Text(student.name)
//                .font(.largeTitle)
//                .padding(.top)
//            
//            if let image = student.image {
//                Image(nsImage: image)
//                    .resizable()
//                    .aspectRatio(contentMode: .fit)
//                    .frame(maxWidth: .infinity, maxHeight: .infinity)
//            } else {
//                Rectangle()
//                    .fill(Color.secondary.opacity(0.2))
//                    .aspectRatio(16/9, contentMode: .fit)
//                    .overlay(
//                        Image(systemName: "display")
//                            .font(.system(size: 50))
//                            .foregroundColor(.secondary)
//                    )
//            }
//            
//            HStack {
//                Button(action: {}) {
//                    Label("Send Message", systemImage: "message")
//                        .padding(.horizontal)
//                }
//                .buttonStyle(BorderedButtonStyle())
//                
//                Button(action: {}) {
//                    Label("Flag Student", systemImage: "flag")
//                        .padding(.horizontal)
//                }
//                .buttonStyle(BorderedButtonStyle())
//                
//                Button(action: {}) {
//                    Label("Disconnect", systemImage: "x.circle")
//                        .padding(.horizontal)
//                }
//                .buttonStyle(BorderedButtonStyle())
//            }
//            .padding(.bottom)
//        }
//        .frame(maxWidth: .infinity, maxHeight: .infinity)
//    }
//}


//
//  DashboardView.swift
//  server_monitor
//
//  Created by Subashanan Nair on 23/03/2025.
//

import SwiftUI

struct DashboardView: View {
    @EnvironmentObject var server: Server
    @State private var gridColumns: [GridItem] = [GridItem(.flexible(), spacing: 16)]
    @State private var selectedStudent: Student? = nil
    @State private var refreshTimer = Timer.publish(every: 1, on: .main, in: .common).autoconnect()

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack(alignment: .center, spacing: 16) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(server.examName.isEmpty ? "Exam Monitoring" : server.examName)
                        .font(.headline)
                    HStack(spacing: 8) {
                        if !server.courseName.isEmpty {
                            Text(server.courseName)
                        }
                        if !server.roomNumber.isEmpty {
                            Text("• Room \(server.roomNumber)")
                        }
                        Text("• \(server.students.count) connected")
                    }
                    .font(.subheadline)
                    .foregroundColor(.secondary)
                }

                if !server.joinCode.isEmpty {
                    VStack(alignment: .leading, spacing: 1) {
                        Text("Join code")
                            .font(.caption2)
                            .foregroundColor(.secondary)
                        Text(server.joinCode)
                            .font(.system(.title3, design: .monospaced))
                            .fontWeight(.semibold)
                            .tracking(2)
                    }
                    .padding(.horizontal, 10)
                    .padding(.vertical, 4)
                    .background(Color.blue.opacity(0.12))
                    .cornerRadius(6)
                }

                Spacer()

                // Column adjustment controls
                HStack(spacing: 12) {
                    Button(action: {
                        updateGridColumns(count: max(gridColumns.count - 1, 1))
                    }) {
                        Image(systemName: "minus.circle")
                            .font(.title3)
                    }
                    .buttonStyle(PlainButtonStyle())
                    .disabled(gridColumns.count <= 1)

                    Text("\(gridColumns.count) column\(gridColumns.count == 1 ? "" : "s")")
                        .font(.subheadline)
                        .frame(minWidth: 70)

                    Button(action: {
                        updateGridColumns(count: min(gridColumns.count + 1, 6))
                    }) {
                        Image(systemName: "plus.circle")
                            .font(.title3)
                    }
                    .buttonStyle(PlainButtonStyle())
                    .disabled(gridColumns.count >= 6)
                }

                Button(action: { server.stop() }) {
                    Text("Stop Monitoring")
                        .font(.subheadline)
                        .fontWeight(.semibold)
                        .foregroundColor(.white)
                        .padding(.horizontal, 16)
                        .frame(height: 32)
                        .background(Color.red)
                        .cornerRadius(8)
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 12)
            .background(Color(NSColor.windowBackgroundColor))
            .overlay(
                Rectangle()
                    .frame(height: 1)
                    .foregroundColor(Color(NSColor.separatorColor)),
                alignment: .bottom
            )
            
            // Student grid
            if server.students.isEmpty {
                VStack {
                    Spacer()
                    Text("Waiting for students to connect...")
                        .font(.title2)
                        .foregroundColor(.secondary)
                    Spacer()
                }
                .onAppear {
                    print("SERVER UI: Empty students view appeared")
                }
            } else {
                ScrollView {
                    LazyVGrid(columns: gridColumns, spacing: 16) {
                        ForEach(server.students) { student in
                            StudentThumbnail(student: student, isSelected: selectedStudent?.id == student.id)
                                .onTapGesture {
                                    selectedStudent = student
                                }
                        }
                    }
                    .padding()
                    .onAppear {
                        print("SERVER UI: Grid with \(server.students.count) students appeared")
                        for (i, student) in server.students.enumerated() {
                            print("SERVER UI: Student \(i): \(student.name), has image: \(student.image != nil)")
                        }
                    }
                }
            }
        }
        .sheet(item: $selectedStudent) { student in
            // Look up by id inside the sheet so the feed stays live,
            // not a frozen copy of the struct captured at tap time.
            StudentDetailView(studentID: student.id, fallback: student)
                .environmentObject(server)
                .frame(width: 800, height: 600)
        }
        // Add debugging modifiers
        .onAppear {
            print("SERVER UI: Dashboard appeared, \(server.students.count) students")
        }
        .onChange(of: server.students.count) {oldCOunt, newCount in
            print("SERVER UI: Student count changed to: \(newCount)")
        }
        // Add timer to force refresh
        .onReceive(refreshTimer) { _ in
            // This will force the view to refresh
            if !server.students.isEmpty {
                print("SERVER UI: Refresh timer fired, \(server.students.count) students")
                for (i, student) in server.students.enumerated() {
                    print("SERVER UI: Student \(i): \(student.name), has image: \(student.image != nil)")
                }
            }
        }
    }
    
    private func updateGridColumns(count: Int) {
        gridColumns = Array(repeating: GridItem(.flexible(), spacing: 16), count: count)
    }
}

struct StudentThumbnail: View {
    let student: Student
    let isSelected: Bool
    
    var body: some View {
        VStack {
            if let image = student.image {
                Image(nsImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
                    .frame(minHeight: 160)
                    .cornerRadius(4)
                    .overlay(
                        RoundedRectangle(cornerRadius: 4)
                            .stroke(isSelected ? Color.blue : Color.clear, lineWidth: 3)
                    )
            } else {
                Rectangle()
                    .fill(Color.secondary.opacity(0.2))
                    .aspectRatio(16/9, contentMode: .fit)
                    .overlay(
                        Image(systemName: "display")
                            .font(.system(size: 30))
                            .foregroundColor(.secondary)
                    )
                    .cornerRadius(4)
            }
            
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(student.name)
                        .font(.headline)
                        .lineLimit(1)
                    if !student.studentID.isEmpty {
                        Text(student.studentID)
                            .font(.caption)
                            .foregroundColor(.secondary)
                            .lineLimit(1)
                    }
                }

                Spacer()

                Text(timeAgoString(from: student.lastUpdate))
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
        }
        .padding(8)
        .background(Color(NSColor.controlBackgroundColor))
        .cornerRadius(8)
    }
    
    private func timeAgoString(from date: Date) -> String {
        let seconds = Int(Date().timeIntervalSince(date))
        
        if seconds < 15 {
            return "Just now"
        } else if seconds < 60 {
            return "\(seconds)s ago"
        } else if seconds < 3600 {
            return "\(seconds / 60)m ago"
        } else {
            return "\(seconds / 3600)h ago"
        }
    }
}

struct StudentDetailView: View {
    @EnvironmentObject var server: Server
    let studentID: UUID
    let fallback: Student

    private var student: Student {
        server.students.first { $0.id == studentID } ?? fallback
    }

    var body: some View {
        VStack(spacing: 20) {
            VStack(spacing: 4) {
                Text(student.name)
                    .font(.largeTitle)
                if !student.studentID.isEmpty {
                    Text(student.studentID)
                        .font(.title3)
                        .foregroundColor(.secondary)
                }
            }
            .padding(.top)

            if let image = student.image {
                Image(nsImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                Rectangle()
                    .fill(Color.secondary.opacity(0.2))
                    .aspectRatio(16/9, contentMode: .fit)
                    .overlay(
                        Image(systemName: "display")
                            .font(.system(size: 50))
                            .foregroundColor(.secondary)
                    )
            }
            
            HStack {
                Button(action: {}) {
                    Label("Send Message", systemImage: "message")
                        .padding(.horizontal)
                }
                .buttonStyle(BorderedButtonStyle())
                
                Button(action: {}) {
                    Label("Flag Student", systemImage: "flag")
                        .padding(.horizontal)
                }
                .buttonStyle(BorderedButtonStyle())
                
                Button(action: {}) {
                    Label("Disconnect", systemImage: "x.circle")
                        .padding(.horizontal)
                }
                .buttonStyle(BorderedButtonStyle())
            }
            .padding(.bottom)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
