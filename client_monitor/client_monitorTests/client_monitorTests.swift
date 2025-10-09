//
//  client_monitorTests.swift
//  client_monitorTests
//
//  Created by Subashanan Nair on 22/03/2025.
//

//
//  client_monitorTests.swift
//  client_monitorTests
//
//  Created by Subashanan Nair on 22/03/2025.
//

import Testing
@testable import client_monitor
import Network

struct client_monitorTests {

    @Test func testDiscoverServerRaceCondition() {
        let client = Client()

        // This will attempt to connect to a non-existent server.
        // Before the fix, this could crash due to a race condition
        // where group.leave() is called multiple times.
        // With the fix, it should gracefully fail and return nil.
        let serverAddress = client.discoverServer(port: 12345)

        #expect(serverAddress == nil)
    }

}
