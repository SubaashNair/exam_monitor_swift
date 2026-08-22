//
//  server_monitorTests.swift
//  server_monitorTests
//
//  Created by Subashanan Nair on 23/03/2025.
//

import Foundation
import Testing
@testable import server_monitor

struct server_monitorTests {

    @Test("Discovery responder answers a UDP discover probe with server")
    func respondsToDiscoveryProbe() throws {
        let port: UInt16 = 42_353
        let responder = UDPDiscoveryResponder(port: port)
        responder.start()
        defer { responder.stop() }

        // Give the responder thread a moment to start listening.
        Thread.sleep(forTimeInterval: 0.2)

        let probeFD = Darwin.socket(AF_INET, SOCK_DGRAM, 0)
        try #require(probeFD >= 0)
        defer { close(probeFD) }

        var timeout = timeval(tv_sec: 3, tv_usec: 0)
        setsockopt(probeFD, SOL_SOCKET, SO_RCVTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size))

        var target = sockaddr_in()
        target.sin_family = sa_family_t(AF_INET)
        target.sin_port = port.bigEndian
        target.sin_addr.s_addr = inet_addr("127.0.0.1")

        let probe = Array("discover".utf8)
        let sent = withUnsafePointer(to: &target) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                sendto(probeFD, probe, probe.count, 0, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        #expect(sent == probe.count)

        var buffer = [UInt8](repeating: 0, count: 64)
        var source = sockaddr_in()
        var sourceLength = socklen_t(MemoryLayout<sockaddr_in>.size)
        let received = withUnsafeMutablePointer(to: &source) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                recvfrom(probeFD, &buffer, buffer.count, 0, $0, &sourceLength)
            }
        }

        try #require(received > 0)
        #expect(String(decoding: buffer[0..<received], as: UTF8.self) == "server")
        #expect(UInt16(bigEndian: source.sin_port) == port)
    }

    @Test("Broadcast targets always include the limited broadcast address")
    func broadcastTargetsIncludeLimitedBroadcast() {
        let targets = discoveryBroadcastAddresses()
        #expect(targets.contains("255.255.255.255"))
        #expect(Set(targets).count == targets.count)
    }

    /// Cross-language contract: these are the same vectors asserted by
    /// `port_vectors_match_the_swift_implementation` in the Rust core. If this
    /// fails, Swift and Tauri apps can no longer find each other's rooms.
    @Test("Port derivation matches the Rust core")
    func portForCodeMatchesRustVectors() {
        #expect(portForCode("N7KU") == 38_242)
        #expect(portForCode("MATH") == 34_703)
        #expect(portForCode("AAAA") == 37_697)
    }

    @Test("Port derivation ignores case and padding, stays in range")
    func portForCodeNormalises() {
        #expect(portForCode("math") == portForCode("MATH"))
        #expect(portForCode("  MATH \n") == portForCode("MATH"))
        for code in ["N7KU", "MATH", "AAAA", "ZZ99", "7GK4"] {
            let port = portForCode(code)
            #expect(port >= 20_000 && port <= 44_999)
        }
    }

    @Test("Join code is 4 chars from the unambiguous alphabet")
    func joinCodeFormat() {
        let allowed = Set("ABCDEFGHJKLMNPQRSTUVWXYZ23456789")
        for _ in 0..<50 {
            let code = generateJoinCode()
            #expect(code.count == 4)
            #expect(code.allSatisfy { allowed.contains($0) })
        }
    }

}
