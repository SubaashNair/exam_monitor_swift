//
//  Untitled.swift
//  server_monitor
//
//  Created by Subashanan Nair on 23/03/2025.
//

import Foundation
import Network
import Cocoa
import Combine

enum DataType: UInt16 {
    case name = 0
    case message = 1
    case picture = 2
}

struct Student: Identifiable {
    let id: UUID
    var name: String
    var image: NSImage?
    var connection: NWConnection
    var lastUpdate: Date
    
    init(connection: NWConnection) {
        self.id = UUID()
        self.name = "Unknown"
        self.image = nil
        self.connection = connection
        self.lastUpdate = Date()
    }
}

class Server: NSObject, ObservableObject {
    private let headerSize = 8
    private var tcpListener: NWListener?
    private var udpSocket: NWConnection?
    private let broadcastTimer = Timer.publish(every: 2.0, on: .main, in: .common).autoconnect()
    private var broadcastCancellable: AnyCancellable?
    private var port: Int = 0
    
    @Published var students: [Student] = []
    @Published var isRunning: Bool = false
    
    func start(port: Int) {
        guard !isRunning else { return }
        
        self.port = port
        setupTCPListener(port: port)
        startUDPBroadcast(port: port)
        
        isRunning = true
    }
    
    func stop() {
        guard isRunning else { return }
        
        // Stop the TCP listener
        tcpListener?.cancel()
        tcpListener = nil
        
        // Stop the UDP broadcast
        broadcastCancellable?.cancel()
        udpSocket?.cancel()
        udpSocket = nil
        
        // Disconnect all students
        for student in students {
            student.connection.cancel()
        }
        
        DispatchQueue.main.async {
            self.students.removeAll()
            self.isRunning = false
        }
    }
    
    // MARK: - Private Methods
    
    private func setupTCPListener(port: Int) {
        do {
            // Create TCP parameters with options
            let parameters = NWParameters.tcp
            parameters.allowLocalEndpointReuse = true
            
            // Create TCP listener with parameters
            let tcpPort = NWEndpoint.Port(integerLiteral: UInt16(port))
            let listener = try NWListener(using: parameters, on: tcpPort)
            
            // Configure listener
            listener.stateUpdateHandler = { [weak self] state in
                switch state {
                case .ready:
                    print("Server ready on port \(port)")
                case .failed(let error):
                    print("Server listener failed: \(error)")
                    DispatchQueue.main.async {
                        self?.stop()
                    }
                default:
                    break
                }
            }
            
            // Handle new connections
            listener.newConnectionHandler = { [weak self] connection in
                guard let self = self else { return }
                
                connection.stateUpdateHandler = { [weak self] state in
                    switch state {
                    case .ready:
                        // Add student to the list
                        DispatchQueue.main.async {
                            let student = Student(connection: connection)
                            self?.students.append(student)
                            self?.receiveData(from: student)
                        }
                    case .failed, .cancelled:
                        // Remove student from the list
                        DispatchQueue.main.async {
                            self?.removeStudent(with: connection)
                        }
                    default:
                        break
                    }
                }
                
                connection.start(queue: DispatchQueue.global())
            }
            
            listener.start(queue: DispatchQueue.global())
            tcpListener = listener
            
        } catch {
            print("Failed to create TCP listener: \(error)")
        }
    }
    
    private func startUDPBroadcast(port: Int) {
        // Start periodic broadcasting
        broadcastCancellable = broadcastTimer.sink { [weak self] _ in
            guard let self = self, self.isRunning else { return }
            self.sendBroadcast()
        }
    }
    
    private func sendBroadcast() {
        // Create a simple broadcast message
        let serverInfo = "server".data(using: .utf8)!
        print("SERVER: Sending broadcast message")
        
        // Use multiple broadcast techniques to increase chances of success
        let broadcastAddresses = ["255.255.255.255", "192.168.1.255", "10.0.0.255"]
        for address in broadcastAddresses {
            let broadcastHost = NWEndpoint.Host(address)
            let broadcastPort = NWEndpoint.Port(integerLiteral: UInt16(port))
            
            // Create UDP parameters
            let parameters = NWParameters.udp
            parameters.allowLocalEndpointReuse = true
            
            let connection = NWConnection(host: broadcastHost, port: broadcastPort, using: parameters)
            connection.start(queue: DispatchQueue.global())
            
            connection.send(content: serverInfo, completion: .contentProcessed { error in
                if let error = error {
                    print("SERVER: Error sending to \(address): \(error)")
                } else {
                    print("SERVER: Broadcast sent to \(address)")
                }
            })
            
            // Cancel after sending (we'll create a new connection next time)
            DispatchQueue.global().asyncAfter(deadline: .now() + 0.5) {
                connection.cancel()
            }
        }
    }
    
    private func receiveData(from student: Student) {
//        guard let index = students.firstIndex(where: { $0.id == student.id }) else { return }
        guard students.contains(where: { $0.id == student.id }) else { return }

        
        // Read header (8 bytes: "HE" + type (2 bytes) + length (4 bytes))
        student.connection.receive(minimumIncompleteLength: headerSize, maximumLength: headerSize) { [weak self] content, _, isComplete, error in
            guard let self = self, let data = content, !isComplete, error == nil else {
                if error != nil || isComplete {
                    DispatchQueue.main.async {
                        self?.removeStudent(with: student.connection)
                    }
                }
                return
            }
            
            // Parse header
            if data.count == self.headerSize && data.prefix(2) == Data("HE".utf8) {
                let type = data.withUnsafeBytes { $0.load(fromByteOffset: 2, as: UInt16.self).bigEndian }
                let length = data.withUnsafeBytes { $0.load(fromByteOffset: 4, as: UInt32.self).bigEndian }
                
                // Read payload
                student.connection.receive(minimumIncompleteLength: Int(length), maximumLength: Int(length)) { [weak self] content, _, _, error in
                    guard let self = self, let payloadData = content, error == nil else {
                        if error != nil {
                            DispatchQueue.main.async {
                                self?.removeStudent(with: student.connection)
                            }
                        }
                        return
                    }
                    
                    // Process the data based on type
                    DispatchQueue.main.async {
                        if let dataType = DataType(rawValue: type) {
                            self.processReceivedData(dataType, data: payloadData, from: student)
                        }
                        // Continue receiving data
                        self.receiveData(from: student)
                    }
                }
            } else {
                // Continue receiving data in case of invalid header
                self.receiveData(from: student)
            }
        }
    }
    
//    private func processReceivedData(_ type: DataType, data: Data, from student: Student) {
//        guard let index = students.firstIndex(where: { $0.id == student.id }) else { return }
//        
//        switch type {
//        case .name:
//            if let name = String(data: data, encoding: .utf8) {
//                students[index].name = name
//            }
//            
//        case .picture:
//            if let nsImage = NSImage(data: data) {
//                students[index].image = nsImage
//                students[index].lastUpdate = Date()
//            }
//            
//        case .message:
//            if let message = String(data: data, encoding: .utf8) {
//                print("Message from \(student.name): \(message)")
//            }
//        }
//    }
//    // In Server.swift - Check your processReceivedData method
//    private func processReceivedData(_ type: DataType, data: Data, from student: Student) {
//        guard let index = students.firstIndex(where: { $0.id == student.id }) else {
//            print("SERVER: Student not found in list")
//            return
//        }
//        
//        switch type {
//        case .name:
//            if let name = String(data: data, encoding: .utf8) {
//                print("SERVER: Received name: \(name)")
//                students[index].name = name
//            }
//            
//        case .picture:
//            if let nsImage = NSImage(data: data) {
//                print("SERVER: Received image: \(data.count) bytes")
//                students[index].image = nsImage
//                students[index].lastUpdate = Date()
//            } else {
//                print("SERVER: Failed to create image from data")
//            }
//            
//        case .message:
//            if let message = String(data: data, encoding: .utf8) {
//                print("SERVER: Message from \(student.name): \(message)")
//            }
//        }
//    }
    
    private func processReceivedData(_ type: DataType, data: Data, from student: Student) {
        guard let index = students.firstIndex(where: { $0.id == student.id }) else {
            print("SERVER: Student not found in list for ID: \(student.id)")
            return
        }
        
        switch type {
        case .name:
            if let name = String(data: data, encoding: .utf8) {
                print("SERVER: Received name: \(name)")
                DispatchQueue.main.async {
                    self.students[index].name = name
                }
            }
            
        case .picture:
            print("SERVER: Processing image data: \(data.count) bytes")
            if let nsImage = NSImage(data: data) {
                print("SERVER: Successfully created NSImage: \(nsImage.size.width) x \(nsImage.size.height)")
                DispatchQueue.main.async {
                    self.students[index].image = nsImage
                    self.students[index].lastUpdate = Date()
                    // Force UI refresh by modifying the students array itself
                    let updatedStudent = self.students[index]
                    self.students.remove(at: index)
                    self.students.insert(updatedStudent, at: index)
                }
            } else {
                print("SERVER: Failed to create NSImage from \(data.count) bytes of data")
            }
            
        case .message:
            if let message = String(data: data, encoding: .utf8) {
                print("SERVER: Message from \(student.name): \(message)")
            }
        }
    }
    
//    private func removeStudent(with connection: NWConnection) {
//        students.removeAll { $0.connection === connection }
//    }
    private func removeStudent(with connection: NWConnection) {
        print("SERVER: Removing student. Before: \(students.count) students")
        students.removeAll { $0.connection === connection }
        print("SERVER: After removal: \(students.count) students")
    }
}
