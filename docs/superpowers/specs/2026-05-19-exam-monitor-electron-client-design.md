# Exam Monitor — Cross-Platform Electron Client (Win/Linux)

**Date:** 2026-05-19
**Status:** Design approved; ready for implementation plan
**Author:** SubaashNair (with Claude pair-coding)

## 1. Goal

Provide a Windows + Linux client for Exam Guard that is byte-for-byte protocol-compatible with the existing Swift macOS server. Schools should be able to run the Swift server on a Mac and have students join from Windows or Linux laptops without code or configuration changes on either side.

The Swift macOS client (`exam_monitor_swift/client_monitor`) stays in place and unchanged. The new Electron client targets only Windows and Linux.

## 2. Non-goals (v1)

- Invigilator-to-student messaging.
- Auto-update via `electron-updater` (deferred to v1.1).
- Audio capture.
- Multi-monitor selection UI (default to primary display, matching Swift).
- Code signing infrastructure (signed builds are a separate operational ticket; the design works unsigned for testing and side-loaded distribution).

## 3. Constraints inherited from the Swift implementation

- **Wire protocol:** unchanged. 8-byte big-endian header (`"HE"` + `uint16 type` + `uint32 length`) + payload. Types: `0 = name`, `1 = message`, `2 = picture`.
- **`.name` payload format:** `"name|studentID"`. The server splits on the first `|`. Legacy single-segment payloads (no `|`) still decode with `studentID = ""`.
- **Picture payload:** JPEG bytes. Reference resolution 720 px wide, quality 0.6, cadence ~2 fps (500 ms interval).
- **Discovery:** UDP broadcast on the room number's port. Server broadcasts the literal UTF-8 bytes `"server"` every 2 s to `255.255.255.255`, `192.168.1.255`, and `10.0.0.255`.
- **Connection:** TCP on the same port as discovery (port == room number).

The canonical wire format is documented in `PROTOCOL.md` (to be added to both `exam_monitor_swift` and `exam_monitor_electron`) so future protocol changes require a coordinated edit in both repos.

## 4. Stack & key decisions

- **Electron + TypeScript** for the app.
- **Screen capture:** Chromium's `navigator.mediaDevices.getDisplayMedia()` (same surface used by Teams / Zoom / Discord). JPEG encoding via `OffscreenCanvas` + `canvas.convertToBlob({ type: 'image/jpeg', quality: 0.6 })`.
- **Networking:** Node's built-in `net` (TCP) and `dgram` (UDP) in the main process. No third-party networking dependencies.
- **UI:** React + TypeScript for the renderer. Matches the dark-themed card-based UI of the Swift client.
- **Build:** `electron-builder` produces a Windows NSIS installer (`.exe`), a Linux AppImage, and a Linux `.deb` from one source tree.

### Why Electron (vs Tauri / Go-Fyne)

Screen capture is the riskiest subsystem (see the SCStream stall we just fixed in Swift). Chromium has spent a decade hardening the same code path; it cannot fail the way native screen-capture APIs typically fail. The trade-off is binary size (~120 MB) — acceptable for a school deployment that runs once per exam.

## 5. Architecture

Two processes, sharply separated:

### 5.1 Main process — networking & lifecycle

| File | Responsibility |
| ---- | -------------- |
| `src/main/main.ts` | App lifecycle, window creation, registers Linux Wayland flag. |
| `src/main/discovery.ts` | UDP broadcast probe on the room's port. 5 s timeout, then emits `discovery:timeout` for the renderer to show the manual-IP fallback. |
| `src/main/connection.ts` | TCP client socket. Packet encoder (`HE` + type BE + length BE + payload). Reconnect loop with exponential backoff (1 → 2 → 4 → 8 s, capped at 8 s) matching the Swift client. |
| `src/main/ipc.ts` | Typed `contextBridge` channels. |

### 5.2 Renderer process — UI & screen capture

| File | Responsibility |
| ---- | -------------- |
| `src/renderer/main.tsx` | React mount, routes between Join and Sharing views. |
| `src/renderer/views/Join.tsx` | Form fields: name, Student ID, room. After a discovery timeout, an additional optional IP field appears. Validation matches Swift: name non-empty, ID ≥ 3 chars after trim, room a positive integer (1–65535). |
| `src/renderer/views/Sharing.tsx` | Status card with name / ID / room / connection state and a Stop Sharing button. Pixel-equivalent to the Swift redesign. |
| `src/renderer/capture.ts` | Holds the `MediaStream` from `getDisplayMedia`, draws each frame onto an `OffscreenCanvas` resized to 720 px wide, encodes as JPEG (quality 0.6), sends the bytes to main over IPC every 500 ms. |

### 5.3 IPC channel surface

| Channel | Direction | Payload |
| ------- | --------- | ------- |
| `client:start` | renderer → main | `{ name: string, studentID: string, room: number, manualIP?: string }` |
| `client:stop` | renderer → main | `{}` |
| `client:frame` | renderer → main | `ArrayBuffer` (JPEG bytes) |
| `status:update` | main → renderer | `{ state: 'discovering' \| 'connecting' \| 'connected' \| 'disconnected' \| 'discoveryTimeout' \| 'error', detail?: string }` |
| `capture:request` | main → renderer | `{}` — tells the renderer it's now safe to call `getDisplayMedia` |

## 6. Data flow — happy path

1. User fills the Join form and clicks Start.
2. Renderer sends `client:start` with `{ name, studentID, room, manualIP?: undefined }`.
3. Main starts UDP discovery on `room` port. If a `"server"` broadcast arrives within 5 s, main records the source IP and proceeds. Otherwise it emits `status:update { state: 'discoveryTimeout' }` and waits — the renderer shows the manual IP field and re-issues `client:start` once the user submits an IP.
4. Main opens a TCP socket to the resolved IP:room. On `connect`, main sends the `.name` packet with payload `"name|studentID"` (UTF-8 encoded).
5. Main emits `status:update { state: 'connected' }` and sends `capture:request`.
6. Renderer calls `getDisplayMedia({ video: { displaySurface: 'monitor' } })`. On user grant, it starts a 500 ms `setInterval` that:
   1. Draws the current `MediaStreamTrack` frame onto a 720 px-wide `OffscreenCanvas` (height preserves aspect).
   2. Calls `canvas.convertToBlob({ type: 'image/jpeg', quality: 0.6 })`.
   3. Reads the blob as an `ArrayBuffer` and posts it over `client:frame`.
7. Main wraps each `ArrayBuffer` in the 8-byte header (`type = 2`, `length = byteLength`) and writes it to the TCP socket.

## 7. Error handling

| Failure | Behaviour |
| ------- | --------- |
| Discovery timeout (5 s, no broadcast received) | Main emits `status:update { state: 'discoveryTimeout' }`. Renderer reveals a manual IP input and a "Try again" button. |
| TCP socket `error` or `close` while sharing | Main emits `status:update { state: 'disconnected' }`, then re-enters the reconnect loop with backoff 1 → 2 → 4 → 8 s. Renderer shows "Reconnecting…" but keeps the capture stream alive so reconnection is instant. |
| User denies screen-capture permission | Renderer shows a clear error card with a Retry button that re-invokes `getDisplayMedia`. |
| `getDisplayMedia` rejects on Linux Wayland | Caused by missing PipeWire feature flag. Main always launches Electron with `--enable-features=WebRTCPipeWireCapturer`; document in README that on Wayland the user will see an xdg-desktop-portal prompt. |
| Server closes the connection cleanly (e.g. invigilator stopped) | Renderer returns to Join view; status pill turns grey. |
| User clicks Stop Sharing | Renderer sends `client:stop`. Main cancels reconnect, closes the socket, stops the capture interval, returns the renderer to Join view. |

## 8. Testing strategy

### 8.1 Unit
- **Packet encoder fixtures.** Capture once with a hex dump from the Swift client (e.g. send a name `"Ada|STU-1"` and a picture of a known JPEG). Hard-code the byte arrays as golden files. Assert the Electron encoder produces identical bytes for the same inputs.
- **`name|studentID` parser symmetry.** The encoder builds the payload; a paired decoder (used only in tests, mirroring the Swift `processReceivedData` logic) recovers the original fields.

### 8.2 Integration
- A Node-side mock server in the test suite listens on a random port, accepts one connection, reads the first `.name` packet, returns a known image, and asserts the byte-level round-trip matches expectations.
- A second integration test exercises discovery: a mock UDP broadcaster sends `"server"` once, and the client should resolve to that IP within 5 s.

### 8.3 Manual matrix
Run each combination against the macOS Swift server in this repo and confirm frames stream with *varying* JPEG byte sizes (the freeze-detection signal we used during the Swift fix):

- Win 10 ↔ macOS Swift server
- Win 11 ↔ macOS Swift server
- Ubuntu 22.04 LTS, Wayland session ↔ macOS Swift server
- Ubuntu 22.04 LTS, X11 session ↔ macOS Swift server

## 9. Repo placement

**New repository** `exam_monitor_electron`, alongside `exam_monitor_swift`. The Swift repo is not touched by this work.

A `PROTOCOL.md` file is added to **both** repos describing the canonical wire format. Any future change to the protocol (e.g. adding a new packet type) requires coordinated PRs in both repos.

### File layout

```
exam_monitor_electron/
├── PROTOCOL.md
├── README.md
├── package.json
├── electron-builder.yml
├── tsconfig.json
├── src/
│   ├── main/
│   │   ├── main.ts
│   │   ├── discovery.ts
│   │   ├── connection.ts
│   │   ├── ipc.ts
│   │   └── preload.ts
│   └── renderer/
│       ├── main.tsx
│       ├── capture.ts
│       ├── ipc-bridge.ts
│       └── views/
│           ├── Join.tsx
│           └── Sharing.tsx
├── test/
│   ├── encoder.test.ts
│   ├── discovery.test.ts
│   └── fixtures/
│       └── golden.bin
└── docs/
    └── images/
```

## 10. Open questions

None blocking. The following can be decided during implementation without redesigning:

- Exact React state-management approach (`useState` + a thin context is likely sufficient given the small surface).
- Whether to use `webpack`, `vite`, or `esbuild` for the renderer bundle. Recommendation: **Vite + electron-vite plugin** for faster iteration; not load-bearing.
- Whether to ship a system-tray icon. Default to no for v1.

## 11. References

- `exam_monitor_swift/client_monitor/client_monitor/Client.swift` — packet format, discovery behaviour, backoff curve.
- `exam_monitor_swift/server_monitor/server_monitor/Server.swift` — `.name` parser (split-on-`|`), `processReceivedData` for picture handling.
- Earlier session's SCStream stall fix (commit `9e0c71b`) — the lessons here informed the choice to delegate screen capture to Chromium rather than re-implement it.
