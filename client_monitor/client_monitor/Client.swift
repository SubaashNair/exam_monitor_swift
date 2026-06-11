//
//  Client.swift
//  client_monitor
//
//  Created by Subashanan Nair on 22/03/2025.
//

import Foundation
import Cocoa
import Network
import ScreenCaptureKit
import AVFoundation
import ImageIO
import UniformTypeIdentifiers

enum PacketType: UInt16 {
    case name = 0
    case message = 1
    case picture = 2
}

class Client: NSObject {
    private var _isRunning = false
    private var socket: NWConnection?
    private var updateUI: (() -> Void)?
    private let headerSize = 8
    private let updateInterval: TimeInterval = 0.5 // 2 frames per second
    private var captureTimer: Timer?
    private var screenCaptureManager = ScreenCaptureManager()
    
    // Public properties
    @Published var isConnected: Bool = false
    var isRunning: Bool { _isRunning }
    
    private var studentName: String = ""
    private var studentID: String = ""

    func start(studentName: String, studentID: String, port: Int, updateUI: @escaping () -> Void) {
        self.updateUI = updateUI
        self.studentName = studentName
        self.studentID = studentID
        _isRunning = true

        // Initialize screen capture
        Task {
            do {
                try await screenCaptureManager.prepareCapture()
            } catch {
                print("Failed to prepare screen capture: \(error)")
            }
        }

        DispatchQueue.global(qos: .background).async { [weak self] in
            guard let self = self else { return }

            var retryDelay: TimeInterval = 1.0

            while self._isRunning {
                self.isConnected = false
                DispatchQueue.main.async {
                    updateUI()
                }

                // Discover server
                if let serverAddress = self.discoverServer(port: port) {
                    self.isConnected = true
                    DispatchQueue.main.async {
                        updateUI()
                    }
                    retryDelay = 1.0

                    // Connect to server
                    let host = NWEndpoint.Host(serverAddress)
                    let nwPort = NWEndpoint.Port(integerLiteral: UInt16(port))

                    let parameters = NWParameters.tcp
                    parameters.allowLocalEndpointReuse = true

                    self.socket = NWConnection(host: host, port: nwPort, using: parameters)

                    self.socket?.stateUpdateHandler = { [weak self] state in
                        guard let self = self else { return }
                        switch state {
                        case .ready:
                            // Send "name|studentID" in the existing name packet
                            self.sendIdentity(name: self.studentName, studentID: self.studentID)
                            
                            // Start capturing screen
                            DispatchQueue.main.async {
                                self.captureTimer = Timer.scheduledTimer(
                                    timeInterval: self.updateInterval,
                                    target: self,
                                    selector: #selector(self.captureAndSendScreen),
                                    userInfo: nil,
                                    repeats: true
                                )
                            }
                        case .failed, .cancelled:
                            self.isConnected = false
                            DispatchQueue.main.async {
                                updateUI()
                            }
                        default:
                            break
                        }
                    }
                    
                    self.socket?.start(queue: DispatchQueue.global())
                    
                    // Wait for connection to end
                    while self.isConnected && self._isRunning {
                        Thread.sleep(forTimeInterval: 0.1)
                    }
                    
                    // Clean up
                    DispatchQueue.main.async {
                        self.captureTimer?.invalidate()
                        self.captureTimer = nil
                    }
                    self.socket?.cancel()
                    self.socket = nil
                } else {
                    // Server discovery failed, retry after delay
                    Thread.sleep(forTimeInterval: retryDelay)
                    retryDelay = min(retryDelay * 2, 8.0)
                }
            }
        }
    }
    
    func stop() {
        _isRunning = false
        isConnected = false
        
        DispatchQueue.main.async {
            self.captureTimer?.invalidate()
            self.captureTimer = nil
        }
        
        socket?.cancel()
        socket = nil
        
        Task {
            await screenCaptureManager.stopCapture()
        }
    }
    
    // MARK: - Private Methods
    
    private func discoverServer(port: Int) -> String? {
        // Fast path: server running on this machine.
        if canConnectDirectly(to: "127.0.0.1", port: port) {
            return "127.0.0.1"
        }

        // UDP probe + listen. The client broadcasts "discover" to every local
        // subnet; the server answers "server" directly to us. We also bind to
        // the room port so the server's periodic beacons reach us.
        let socketFD = Darwin.socket(AF_INET, SOCK_DGRAM, 0)
        guard socketFD >= 0 else {
            print("CLIENT: failed to create UDP discovery socket: \(String(cString: strerror(errno)))")
            return nil
        }
        defer { close(socketFD) }

        var enable: Int32 = 1
        if setsockopt(socketFD, SOL_SOCKET, SO_BROADCAST, &enable, socklen_t(MemoryLayout<Int32>.size)) < 0 {
            print("CLIENT: failed to enable UDP broadcast: \(String(cString: strerror(errno)))")
        }
        setsockopt(socketFD, SOL_SOCKET, SO_REUSEADDR, &enable, socklen_t(MemoryLayout<Int32>.size))

        var timeout = timeval(tv_sec: 1, tv_usec: 0)
        setsockopt(socketFD, SOL_SOCKET, SO_RCVTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size))

        // Bind to the room port to hear beacons; probe replies arrive either way.
        var localAddress = sockaddr_in()
        localAddress.sin_family = sa_family_t(AF_INET)
        localAddress.sin_port = UInt16(port).bigEndian
        localAddress.sin_addr.s_addr = INADDR_ANY
        let bindResult = withUnsafePointer(to: &localAddress) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.bind(socketFD, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        if bindResult < 0 {
            print("CLIENT: UDP bind on port \(port) failed (\(String(cString: strerror(errno)))); using ephemeral port")
        }

        let targets = discoveryBroadcastAddresses()
        print("CLIENT: probing for server on \(targets)")

        var buffer = [UInt8](repeating: 0, count: 64)

        // Probe roughly once per second for ~10 seconds, listening in between.
        for _ in 0..<10 where _isRunning {
            for address in targets {
                sendDatagram("discover", toAddress: address, port: UInt16(port), via: socketFD)
            }

            var source = sockaddr_in()
            var sourceLength = socklen_t(MemoryLayout<sockaddr_in>.size)
            let count = withUnsafeMutablePointer(to: &source) {
                $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                    recvfrom(socketFD, &buffer, buffer.count, 0, $0, &sourceLength)
                }
            }

            if count > 0, String(decoding: buffer[0..<count], as: UTF8.self) == "server" {
                let serverAddress = String(cString: inet_ntoa(source.sin_addr))
                print("CLIENT: discovered server at \(serverAddress)")
                return serverAddress
            }
        }

        print("CLIENT: discovery timed out")
        return nil
    }

    private func canConnectDirectly(to address: String, port: Int) -> Bool {
        let host = NWEndpoint.Host(address)
        let nwPort = NWEndpoint.Port(integerLiteral: UInt16(port))
        let parameters = NWParameters.tcp
        parameters.allowLocalEndpointReuse = true

        let connection = NWConnection(host: host, port: nwPort, using: parameters)
        let group = DispatchGroup()
        group.enter()

        var isServerFound = false
        connection.stateUpdateHandler = { state in
            switch state {
            case .ready:
                isServerFound = true
                group.leave()
            case .failed, .cancelled:
                if !isServerFound {
                    group.leave()
                }
            default:
                break
            }
        }

        connection.start(queue: DispatchQueue.global())
        let result = group.wait(timeout: .now() + 0.5) == .success && isServerFound
        connection.cancel()
        return result
    }

    private func sendDatagram(_ message: String, toAddress address: String, port: UInt16, via socketFD: Int32) {
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
            print("CLIENT: UDP probe to \(address):\(port) failed: \(String(cString: strerror(errno)))")
        }
    }
    
    @objc private func captureAndSendScreen() {
        guard isConnected else { return }
        
        // Create a separate function that will be called from the task
        func captureAndProcess() async {
            do {
                if let imageData = try await screenCaptureManager.captureScreenAsJPEG(quality: 0.6, width: 720) {
                    let dataCopy = imageData // Create a local copy that doesn't reference self
                    // Use the main actor to safely access sendScreenshot
                    await MainActor.run {
                        self.sendScreenshot(dataCopy)
                    }
                }
            } catch {
                print("Failed to capture screen: \(error)")
            }
        }
        
        // Create the task without capturing self directly in the closure
        _ = Task {
            await captureAndProcess()
        }
    }
    
    private func sendIdentity(name: String, studentID: String) {
        // Payload format: "name|studentID". Server splits on '|';
        // legacy single-segment payloads still parse correctly (id stays empty).
        let payload = "\(name)|\(studentID)"
        guard let data = payload.data(using: .utf8) else { return }
        sendData(type: .name, data: data)
    }
    
    private func sendScreenshot(_ screenshot: Data) {
        sendData(type: .picture, data: screenshot)
        print("Screenshot sent: \(screenshot.count) bytes")
    }
    
    private func sendMessage(_ message: String) {
        guard let data = message.data(using: .utf8) else {
            return
        }
        
        sendData(type: .message, data: data)
    }
    
    private func sendData(type: PacketType, data: Data) {
        var header = Data(capacity: headerSize)
        
        // Add "HE" magic bytes
        header.append(contentsOf: "HE".utf8)
        
        // Add packet type (2 bytes, big endian)
        var typeValue = type.rawValue.bigEndian
        withUnsafeBytes(of: &typeValue) { header.append(contentsOf: $0) }
        
        // Add data length (4 bytes, big endian)
        var length = UInt32(data.count).bigEndian
        withUnsafeBytes(of: &length) { header.append(contentsOf: $0) }
        
        // Combine header and data
        var packet = Data(capacity: header.count + data.count)
        packet.append(header)
        packet.append(data)
        
        // Send packet
        socket?.send(content: packet, completion: .contentProcessed { [weak self] error in
            if let error = error {
                print("Error sending data: \(error)")
                self?.isConnected = false
                if let updateUI = self?.updateUI {
                    DispatchQueue.main.async {
                        updateUI()
                    }
                }
            }
        })
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

// MARK: - Screen Capture Manager
class ScreenCaptureManager: NSObject, SCStreamOutput, SCStreamDelegate {
    private var stream: SCStream?
    private var latestImage: CGImage?
    private var frameID: UInt64 = 0
    private var lastEncodedFrameID: UInt64 = 0
    private let imageSyncQueue = DispatchQueue(label: "com.client.screencapture.sync")
    private let ciContext = CIContext(options: [.useSoftwareRenderer: false])

    func prepareCapture() async throws {
        let availableContent = try await SCShareableContent.current

        guard let display = availableContent.displays.first else {
            throw NSError(domain: "ScreenCaptureError", code: 1, userInfo: [NSLocalizedDescriptionKey: "No display found"])
        }

        let config = SCStreamConfiguration()
        config.width = 1440
        config.height = 900
        config.minimumFrameInterval = CMTime(value: 1, timescale: 2) // 0.5s
        config.queueDepth = 3
        config.pixelFormat = kCVPixelFormatType_32BGRA
        config.showsCursor = true

        let filter = SCContentFilter(display: display, excludingApplications: [], exceptingWindows: [])

        stream = SCStream(filter: filter, configuration: config, delegate: self)
        try stream?.addStreamOutput(self, type: .screen, sampleHandlerQueue: .global(qos: .userInitiated))
        try await stream?.startCapture()
        print("CLIENT: SCStream startCapture succeeded")
    }

    func stopCapture() async {
        guard let stream = stream else { return }
        do {
            try await stream.stopCapture()
        } catch {
            print("CLIENT: Error stopping capture: \(error)")
        }
        self.stream = nil
    }

    func captureScreenAsJPEG(quality: CGFloat, width: CGFloat) async throws -> Data? {
        for _ in 0..<20 {
            var currentImage: CGImage?
            var currentID: UInt64 = 0
            imageSyncQueue.sync {
                currentImage = latestImage
                currentID = frameID
            }

            if let cgImage = currentImage {
                if currentID == lastEncodedFrameID {
                    print("CLIENT: WARNING — re-encoding stale frame id=\(currentID); SCStream may have stalled")
                } else {
                    print("CLIENT: encoding fresh frame id=\(currentID)")
                }
                lastEncodedFrameID = currentID
                return encodeJPEG(cgImage: cgImage, quality: quality, targetWidth: width)
            }

            try await Task.sleep(nanoseconds: 50_000_000)
        }

        throw NSError(domain: "ScreenCaptureError", code: 2, userInfo: [NSLocalizedDescriptionKey: "Timed out waiting for screen capture"])
    }

    private func encodeJPEG(cgImage: CGImage, quality: CGFloat, targetWidth: CGFloat) -> Data? {
        let originalWidth = CGFloat(cgImage.width)
        let originalHeight = CGFloat(cgImage.height)
        guard originalWidth > 0, originalHeight > 0 else { return nil }

        let aspectRatio = originalHeight / originalWidth
        let outWidth = Int(targetWidth.rounded())
        let outHeight = Int((targetWidth * aspectRatio).rounded())

        let colorSpace = CGColorSpaceCreateDeviceRGB()
        let bitmapInfo: UInt32 = CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue

        guard let context = CGContext(
            data: nil,
            width: outWidth,
            height: outHeight,
            bitsPerComponent: 8,
            bytesPerRow: 0,
            space: colorSpace,
            bitmapInfo: bitmapInfo
        ) else {
            return nil
        }

        context.interpolationQuality = .medium
        context.draw(cgImage, in: CGRect(x: 0, y: 0, width: outWidth, height: outHeight))

        guard let resized = context.makeImage() else { return nil }

        let data = NSMutableData()
        guard let dest = CGImageDestinationCreateWithData(data, UTType.jpeg.identifier as CFString, 1, nil) else {
            return nil
        }
        CGImageDestinationAddImage(dest, resized, [kCGImageDestinationLossyCompressionQuality: quality] as CFDictionary)
        guard CGImageDestinationFinalize(dest) else { return nil }

        return data as Data
    }

    // MARK: - SCStreamOutput
    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen,
              let imageBuffer = CMSampleBufferGetImageBuffer(sampleBuffer) else {
            return
        }

        // Only process frames the system marked as having complete, displayable content.
        if let attachments = CMSampleBufferGetSampleAttachmentsArray(sampleBuffer, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
           let statusRaw = attachments.first?[.status] as? Int,
           let status = SCFrameStatus(rawValue: statusRaw),
           status != .complete {
            return
        }

        let ciImage = CIImage(cvPixelBuffer: imageBuffer)
        guard let cgImage = ciContext.createCGImage(ciImage, from: ciImage.extent) else { return }

        imageSyncQueue.sync {
            latestImage = cgImage
            frameID &+= 1
        }
    }

    // MARK: - SCStreamDelegate
    func stream(_ stream: SCStream, didStopWithError error: Error) {
        print("CLIENT: SCStream stopped with error: \(error)")
    }
}
