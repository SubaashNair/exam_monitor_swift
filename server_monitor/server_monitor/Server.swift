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
    var studentID: String
    var image: NSImage?
    var connection: NWConnection
    var lastUpdate: Date
    /// True once a valid identity (with the correct join code) has arrived.
    var isVerified: Bool

    init(connection: NWConnection) {
        self.id = UUID()
        self.name = "Unknown"
        self.studentID = ""
        self.image = nil
        self.connection = connection
        self.lastUpdate = Date()
        self.isVerified = false
    }
}

/// Generate a 4-character room join code from an unambiguous alphabet
/// (no 0/O/1/I), matching the cross-platform apps.
func generateJoinCode() -> String {
    let alphabet = Array("ABCDEFGHJKLMNPQRSTUVWXYZ23456789")
    return String((0..<4).map { _ in alphabet.randomElement()! })
}

class Server: NSObject, ObservableObject {
    private let headerSize = 8
    private var tcpListener: NWListener?
    private var discoveryResponder: UDPDiscoveryResponder?
    private var staleSweepTimer: Timer?
    private var port: Int = 0

    // Clients stream a frame every 0.5s; no frame for 30s means the laptop
    // slept or Wi-Fi dropped without a TCP reset. Drop it from the dashboard
    // instead of showing a stale "live" tile forever.
    private let staleTimeout: TimeInterval = 30

    @Published var students: [Student] = []
    @Published var isRunning: Bool = false
    @Published var examName: String = ""
    @Published var courseName: String = ""
    @Published var roomNumber: String = ""
    @Published var joinCode: String = ""

    func start(port: Int) {
        guard !isRunning else { return }

        self.port = port
        joinCode = generateJoinCode()
        setupTCPListener(port: port)
        discoveryResponder = UDPDiscoveryResponder(port: UInt16(port))
        discoveryResponder?.start()

        staleSweepTimer = Timer.scheduledTimer(withTimeInterval: 10, repeats: true) { [weak self] _ in
            self?.dropStaleStudents()
        }

        isRunning = true
    }

    private func dropStaleStudents() {
        let cutoff = Date().addingTimeInterval(-staleTimeout)
        let stale = students.filter { $0.lastUpdate < cutoff }
        guard !stale.isEmpty else { return }

        for student in stale {
            print("SERVER: dropping stale student \(student.name) (no frames for \(Int(staleTimeout))s)")
            student.connection.cancel()
        }
        students.removeAll { student in stale.contains { $0.id == student.id } }
    }

    func stop() {
        guard isRunning else { return }

        // Stop the TCP listener
        tcpListener?.cancel()
        tcpListener = nil

        // Stop UDP discovery
        discoveryResponder?.stop()
        discoveryResponder = nil

        staleSweepTimer?.invalidate()
        staleSweepTimer = nil

        // Disconnect all students
        for student in students {
            student.connection.cancel()
        }

        DispatchQueue.main.async {
            self.students.removeAll()
            self.isRunning = false
            self.joinCode = ""
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

                // Reject absurd payload claims (matches the Rust 20MB cap).
                guard length <= 20 * 1024 * 1024 else {
                    print("SERVER: dropping connection claiming \(length) byte payload")
                    student.connection.cancel()
                    DispatchQueue.main.async {
                        self.removeStudent(with: student.connection)
                    }
                    return
                }

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
                // Bad magic means the stream is desynced; there is no way to
                // resync a length-prefixed stream, so drop the connection.
                print("SERVER: invalid packet header, dropping connection")
                student.connection.cancel()
                DispatchQueue.main.async {
                    self.removeStudent(with: student.connection)
                }
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
            if let payload = String(data: data, encoding: .utf8) {
                // Payload is joinCode|name|studentID (matches cross-platform apps).
                let parts = payload.split(separator: "|", maxSplits: 2, omittingEmptySubsequences: false)
                let code = parts.count > 0 ? String(parts[0]) : ""
                let name = parts.count > 1 ? String(parts[1]) : "Unknown"
                let studentID = parts.count > 2 ? String(parts[2]) : ""

                if !joinCode.isEmpty && code != joinCode {
                    print("SERVER: rejecting connection: wrong join code")
                    student.connection.cancel()
                    self.removeStudent(with: student.connection)
                    return
                }

                print("SERVER: Received identity name=\(name) id=\(studentID)")
                self.students[index].name = name
                self.students[index].studentID = studentID
                self.students[index].isVerified = true
            }

        case .picture:
            // A framed room requires the join code first; drop unverified frames.
            if !joinCode.isEmpty && !students[index].isVerified {
                print("SERVER: rejecting frame before join code")
                student.connection.cancel()
                self.removeStudent(with: student.connection)
                return
            }
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

// MARK: - UDP Discovery

/// Answers client "discover" probes and periodically beacons "server" so
/// clients on the local network can find this machine.
///
/// Uses BSD sockets because Network.framework's NWConnection cannot enable
/// SO_BROADCAST, so its sends to broadcast addresses fail.
final class UDPDiscoveryResponder {
    private let port: UInt16
    private var socketFD: Int32 = -1
    private let runningLock = NSLock()
    private var _isRunning = false

    private var isRunning: Bool {
        runningLock.lock()
        defer { runningLock.unlock() }
        return _isRunning
    }

    init(port: UInt16) {
        self.port = port
    }

    func start() {
        socketFD = socket(AF_INET, SOCK_DGRAM, 0)
        guard socketFD >= 0 else {
            print("SERVER: failed to create UDP discovery socket: \(String(cString: strerror(errno)))")
            return
        }

        var enable: Int32 = 1
        if setsockopt(socketFD, SOL_SOCKET, SO_BROADCAST, &enable, socklen_t(MemoryLayout<Int32>.size)) < 0 {
            print("SERVER: failed to enable UDP broadcast: \(String(cString: strerror(errno)))")
        }
        setsockopt(socketFD, SOL_SOCKET, SO_REUSEADDR, &enable, socklen_t(MemoryLayout<Int32>.size))

        // Receive timeout so the loop can check shutdown and send beacons.
        var timeout = timeval(tv_sec: 0, tv_usec: 500_000)
        setsockopt(socketFD, SOL_SOCKET, SO_RCVTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size))

        // Bind to the room port so client "discover" probes reach us. If the
        // port is taken we continue in beacon-only mode.
        var address = sockaddr_in()
        address.sin_family = sa_family_t(AF_INET)
        address.sin_port = port.bigEndian
        address.sin_addr.s_addr = INADDR_ANY
        let bindResult = withUnsafePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                bind(socketFD, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        if bindResult < 0 {
            print("SERVER: UDP bind on port \(port) failed (\(String(cString: strerror(errno)))); beacon-only discovery")
        }

        runningLock.lock()
        _isRunning = true
        runningLock.unlock()

        let thread = Thread { [weak self] in
            self?.runLoop()
        }
        thread.name = "udp-discovery"
        thread.start()
    }

    func stop() {
        runningLock.lock()
        _isRunning = false
        runningLock.unlock()

        if socketFD >= 0 {
            close(socketFD)
            socketFD = -1
        }
    }

    private func runLoop() {
        let targets = discoveryBroadcastAddresses()
        print("SERVER: announcing room \(port) to \(targets)")

        var buffer = [UInt8](repeating: 0, count: 64)
        var lastBeacon = Date.distantPast

        while isRunning {
            if Date().timeIntervalSince(lastBeacon) >= 2.0 {
                for address in targets {
                    sendDatagram("server", toAddress: address, port: port)
                }
                lastBeacon = Date()
            }

            var source = sockaddr_in()
            var sourceLength = socklen_t(MemoryLayout<sockaddr_in>.size)
            let count = withUnsafeMutablePointer(to: &source) {
                $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                    recvfrom(socketFD, &buffer, buffer.count, 0, $0, &sourceLength)
                }
            }

            if count > 0, String(decoding: buffer[0..<count], as: UTF8.self) == "discover" {
                let sourceIP = String(cString: inet_ntoa(source.sin_addr))
                let sourcePort = UInt16(bigEndian: source.sin_port)
                print("SERVER: discovery probe from \(sourceIP):\(sourcePort)")
                sendDatagram("server", toAddress: sourceIP, port: sourcePort)
            } else if count < 0, errno != EAGAIN, errno != EWOULDBLOCK, isRunning {
                print("SERVER: UDP discovery receive failed: \(String(cString: strerror(errno)))")
                Thread.sleep(forTimeInterval: 0.3)
            }
        }
    }

    private func sendDatagram(_ message: String, toAddress address: String, port: UInt16) {
        var target = sockaddr_in()
        target.sin_family = sa_family_t(AF_INET)
        target.sin_port = port.bigEndian
        target.sin_addr.s_addr = inet_addr(address)

        let payload = Array(message.utf8)
        let sent = withUnsafePointer(to: &target) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                sendto(socketFD, payload, payload.count, 0, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        if sent < 0 {
            print("SERVER: UDP send to \(address):\(port) failed: \(String(cString: strerror(errno)))")
        }
    }
}

/// The directed broadcast address of every active non-loopback IPv4
/// interface, plus the limited broadcast address as a fallback.
func discoveryBroadcastAddresses() -> [String] {
    var addresses = ["255.255.255.255"]

    var interfaceList: UnsafeMutablePointer<ifaddrs>?
    guard getifaddrs(&interfaceList) == 0, let first = interfaceList else {
        print("DISCOVERY: failed to enumerate network interfaces")
        return addresses
    }
    defer { freeifaddrs(interfaceList) }

    for pointer in sequence(first: first, next: { $0.pointee.ifa_next }) {
        let interface = pointer.pointee
        let flags = Int32(interface.ifa_flags)
        guard (flags & IFF_UP) != 0,
              (flags & IFF_LOOPBACK) == 0,
              (flags & IFF_BROADCAST) != 0,
              let broadcastPointer = interface.ifa_dstaddr,
              broadcastPointer.pointee.sa_family == sa_family_t(AF_INET) else {
            continue
        }

        var broadcast = sockaddr_in()
        memcpy(&broadcast, broadcastPointer, MemoryLayout<sockaddr_in>.size)
        let address = String(cString: inet_ntoa(broadcast.sin_addr))
        if !addresses.contains(address) {
            addresses.append(address)
        }
    }

    return addresses
}
