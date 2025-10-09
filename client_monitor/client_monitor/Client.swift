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
    
    func start(studentName: String, port: Int, updateUI: @escaping () -> Void) {
        self.updateUI = updateUI
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
                            // Connection established, send student name
                            self.sendStudentName(name: studentName)
                            
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
    
    // MARK: - Internal Methods
    
    func discoverServer(port: Int) -> String? {
        // Try direct connections to common local addresses first
        let potentialServerAddresses = ["localhost", "127.0.0.1", "192.168.1.1", "10.0.0.1"]
        
        for serverAddress in potentialServerAddresses {
            print("CLIENT: Trying direct connection to \(serverAddress)")
            
            // Attempt direct connection
            let host = NWEndpoint.Host(serverAddress)
            let nwPort = NWEndpoint.Port(integerLiteral: UInt16(port))
            let parameters = NWParameters.tcp
            parameters.allowLocalEndpointReuse = true
            
            let connection = NWConnection(host: host, port: nwPort, using: parameters)
            
            // Use a dispatch group to wait for result
            let group = DispatchGroup()
            group.enter()
            
            var isServerFound = false
            
            connection.stateUpdateHandler = { state in
                switch state {
                case .ready:
                    print("CLIENT: Connected directly to \(serverAddress)")
                    isServerFound = true
                    group.leave()  // Only leave the group once
                case .failed, .cancelled:
                    if isServerFound == false {  // Only leave if we haven't already
                        group.leave()
                    }
                default:
                    break
                }
            }
            
            connection.start(queue: DispatchQueue.global())
            
            // Wait briefly for connection attempt
            if group.wait(timeout: .now() + 2.0) == .success && isServerFound {
                connection.cancel()
                return serverAddress
            }
            
            connection.cancel()
        }
        
        // Fall back to UDP discovery
        let group = DispatchGroup()
        var serverAddress: String?
        
        group.enter()
        
        // Create UDP parameters
        let parameters = NWParameters.udp
        parameters.allowLocalEndpointReuse = true
        
        // Use ephemeral port for client
        let clientPort = NWEndpoint.Port(integerLiteral: UInt16(0)) // System will choose available port
        
        do {
            // Create UDP listener
            let listener = try NWListener(using: parameters, on: clientPort)
            
            listener.stateUpdateHandler = { state in
                switch state {
                case .ready:
                    print("CLIENT: UDP listener ready for discovery")
                case .failed(let error):
                    print("CLIENT: UDP listener failed: \(error)")
                    group.leave()
                default:
                    break
                }
            }
            
            listener.newConnectionHandler = { connection in
                print("CLIENT: New connection received during discovery")
                
                connection.receiveMessage { (data, context, isComplete, error) in
                    if let data = data, let message = String(data: data, encoding: .utf8) {
                        print("CLIENT: Received message: \(message)")
                        if message == "server" {
                            // Get the connection endpoint description
                            let endpointString = connection.endpoint.debugDescription.components(separatedBy: ":").first ?? ""
                            
                            // Clean up any IPv6 brackets
                            let cleanAddress = endpointString.replacingOccurrences(of: "[", with: "")
                                .replacingOccurrences(of: "]", with: "")
                                .components(separatedBy: "%").first ?? endpointString
                            
                            print("CLIENT: Found server at \(cleanAddress)")
                            serverAddress = cleanAddress
                            group.leave()
                        }
                    } else if let error = error {
                        print("CLIENT: Error receiving discovery message: \(error)")
                    }
                }
                
                connection.start(queue: DispatchQueue.global())
            }
            
            listener.start(queue: DispatchQueue.global())
            
            print("CLIENT: Waiting for server discovery...")
            
            // Wait for discovery response with timeout
            let waitResult = group.wait(timeout: .now() + 10.0)
            listener.cancel()
            
            if waitResult == .timedOut {
                print("CLIENT: Discovery timed out")
                return nil
            }
            
        } catch {
            print("CLIENT: Failed to create UDP listener: \(error)")
            group.leave()
        }
        
        return serverAddress
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
    
    private func sendStudentName(name: String) {
        guard let data = name.data(using: .utf8) else {
            return
        }
        
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

// MARK: - Screen Capture Manager
class ScreenCaptureManager: NSObject, SCStreamOutput {
    private var stream: SCStream?
    private var latestImage: CGImage?
    private let imageSyncQueue = DispatchQueue(label: "com.client.screencapture.sync")
    
    func prepareCapture() async throws {
        // Get available content to capture
        let availableContent = try await SCShareableContent.current
        
        // Get main display
        guard let display = availableContent.displays.first else {
            throw NSError(domain: "ScreenCaptureError", code: 1, userInfo: [NSLocalizedDescriptionKey: "No display found"])
        }
        
        // Configure capture settings
        let config = SCStreamConfiguration()
        config.width = 1440  // Capture at this resolution, will resize later
        config.height = 900  // Approximate 16:9 aspect ratio
        config.minimumFrameInterval = CMTime(value: 1, timescale: 2)  // 0.5 seconds
        config.queueDepth = 1
        
        // Set up filter to capture the display
        let filter = SCContentFilter(display: display, excludingApplications: [], exceptingWindows: [])
        
        // Create capture stream
        stream = SCStream(filter: filter, configuration: config, delegate: nil)
        
        // Set up stream output
        try stream?.addStreamOutput(self, type: .screen, sampleHandlerQueue: .global())
        
        // Start capture
        try await stream?.startCapture()
    }
    
    func stopCapture() async {
        if let stream = stream {
            do {
                try await stream.stopCapture()
                self.stream = nil
            } catch {
                print("Error stopping capture: \(error)")
            }
        }
    }
    
    func captureScreenAsJPEG(quality: CGFloat, width: CGFloat) async throws -> Data? {
        // Try up to 1 second for a frame if needed
        for _ in 0..<20 {
            var currentImage: CGImage?
            
            // Use dispatch queue instead of locks
            imageSyncQueue.sync {
                currentImage = latestImage
            }
            
            if let cgImage = currentImage {
                return convertToJPEG(cgImage: cgImage, quality: quality, targetWidth: width)
            }
            
            try await Task.sleep(nanoseconds: 50_000_000) // 50ms
        }
        
        throw NSError(domain: "ScreenCaptureError", code: 2, userInfo: [NSLocalizedDescriptionKey: "Timed out waiting for screen capture"])
    }
    
    private func convertToJPEG(cgImage: CGImage, quality: CGFloat, targetWidth: CGFloat) -> Data? {
        // Create NSImage from CGImage
        let originalWidth = CGFloat(cgImage.width)
        let originalHeight = CGFloat(cgImage.height)
        let nsImage = NSImage(cgImage: cgImage, size: NSSize(width: originalWidth, height: originalHeight))
        
        // Calculate new size maintaining aspect ratio
        let aspectRatio = originalHeight / originalWidth
        let targetHeight = targetWidth * aspectRatio
        
        // Resize image
        let resizedImage = NSImage(size: NSSize(width: targetWidth, height: targetHeight))
        resizedImage.lockFocus()
        nsImage.draw(in: NSRect(x: 0, y: 0, width: targetWidth, height: targetHeight),
                    from: NSRect(x: 0, y: 0, width: originalWidth, height: originalHeight),
                    operation: .copy, fraction: 1.0)
        resizedImage.unlockFocus()
        
        // Convert to JPEG
        guard let tiffData = resizedImage.tiffRepresentation,
              let bitmap = NSBitmapImageRep(data: tiffData) else {
            return nil
        }
        
        return bitmap.representation(using: .jpeg, properties: [.compressionFactor: quality])
    }
    
    // MARK: - SCStreamOutput
    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen,
              let imageBuffer = CMSampleBufferGetImageBuffer(sampleBuffer) else {
            return
        }
        
        // Convert to CGImage
        let ciImage = CIImage(cvPixelBuffer: imageBuffer)
        let context = CIContext()
        if let cgImage = context.createCGImage(ciImage, from: ciImage.extent) {
            // Update the latest image using dispatch queue instead of locks
            imageSyncQueue.sync {
                latestImage = cgImage
            }
        }
    }
}
