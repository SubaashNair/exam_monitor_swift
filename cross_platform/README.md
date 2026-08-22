# Exam Monitor Cross-Platform MVP

This workspace ports the Swift app flow to a Rust core plus Tauri desktop shells.

The MVP keeps the same product model:

- Server app creates a monitoring room and generates its class code.
- The class code is the room's only identifier; its network port is derived
  from the code, so nobody types a port.
- Client app joins with student name, student ID, and the class code.
- Client sends identity first.
- Client captures the screen periodically and sends JPEG frames.
- Server dashboard displays connected students and latest screen frames.

## Layout

- `crates/exam-monitor-core`: shared protocol, TCP/UDP networking, screen capture, client runtime, server runtime.
- `apps/client`: Tauri student client.
- `apps/server`: Tauri proctor server.
- `tools/stress-clients`: synthetic TCP clients for load testing the server app.

## Protocol

Packets intentionally match the Swift prototype:

```text
"HE" magic bytes + UInt16 packet type + UInt32 payload length + payload
```

Packet types:

- `0`: identity payload as `joinCode|name|studentID` (since v0.1.10; was
  `name|studentID` before — old 2-field clients are rejected by a v0.1.10 server)
- `1`: message payload, reserved for later
- `2`: JPEG screen frame

## Local Run

Rust is required before this can run:

```bash
cd cross_platform/apps/server
npm install
npm run tauri dev
```

In another terminal:

```bash
cd cross_platform/apps/client
npm install
npm run tauri dev
```

Start the server first, then type the class code it shows into the client.

## How Discovery Works

The client finds the server in three ways, in order:

1. Direct connection to a manually entered server IP (the optional "Server IP" field in the client).
2. UDP probe: the client broadcasts `discover` to every local subnet's broadcast address on the room's port; the server replies `server` directly to the client.
3. Server beacon: the server broadcasts `server` every 2 seconds to the same addresses, and the client listens on that port.

The port is not typed by anyone — both sides compute it from the class code
with the same function (`port_for_code`, mirrored in the Swift apps), so the
code alone is enough to find the room.

The teacher's IP for manual entry is shown by `ipconfig` (Windows) or System Settings → Network (macOS).

## Class Code

From v0.2.0 a room has a single identifier: a 4-character **class code** (e.g.
`7GK4`) generated when the teacher clicks Start and shown on the dashboard.
Students enter that code and nothing else — the port is derived from it, and the
code doubles as the password, so a bystander who only overhears the room cannot
impersonate a classmate.

Read the code out to the class, or show it on a projector, when the exam starts.

### Students on an older client

The wire protocol did not change in v0.2.0, so clients from v0.1.10–v0.1.12
still work. Their join screen asks for a "Class Number" as well: give them the
small `port …` value shown under the code on the dashboard, plus the code
itself. Clients older than v0.1.10 send no code at all and are rejected.

## Session Evidence Logging

When a room starts, the server writes a per-session folder under its app log
directory (macOS: `~/Library/Logs/`; Windows: `%LOCALAPPDATA%\...\logs`; the
exact path is shown at the bottom of the dashboard). The folder contains:

- `events.csv` — an audit trail of every connect / disconnect / reconnect with
  UTC timestamps, name, and student ID.
- Periodic JPEG snapshots of each connected student's screen, saved every 60
  seconds as `<student-id>_<epoch>.jpg`.

Disk use scales with class size and duration: roughly `students × 60` images
per hour (e.g. 30 students for 2 hours ≈ 3,600 small JPEGs). Delete old session
folders when you no longer need the evidence.

Disconnected students are **not** removed from the dashboard — their tile turns
grey ("disconnected Xm ago") so a drop-out stays visible; dismiss it with the ×
on the tile.

To auto-delete a session's screenshots, tick **"Delete this session's
screenshots when I finish"** in the dashboard footer before you finish. It is
off by default and clears the current session's folder when you press Stop
Monitoring or quit the app; older session folders are never touched.

## macOS: "app is damaged and can't be opened"

The apps are not notarized with Apple (that requires a paid developer
account), so macOS quarantines the downloaded zip and can refuse to open the
app. Fixes, in order of preference:

1. After copying the app to Applications, run once in Terminal:

   ```bash
   xattr -cr "/Applications/Exam Guard Client.app"
   ```

2. Or distribute the app on a USB stick — files copied from USB are not
   quarantined, so the app opens normally.

Releases from v0.1.5 onward are ad-hoc signed in CI, which downgrades the
error from "damaged" to an "unidentified developer" warning that can be
bypassed with right-click → Open.

## Network Troubleshooting

If the client stays on "Looking for the server...":

- **Windows Firewall (client and server on Windows):** the first launch shows a firewall prompt — click "Allow access". If it was dismissed, allow the app under Windows Security → Firewall & network protection → Allow an app through firewall, for both Private and Public profiles.
- **macOS Local Network permission (macOS 15+):** the first launch asks for Local Network access — click "Allow". Re-enable later under System Settings → Privacy & Security → Local Network.
- **macOS firewall:** if enabled, allow incoming connections for the server app when prompted.
- **Both machines must be on the same network/subnet.** Guest Wi-Fi and networks with "AP/client isolation" block device-to-device traffic entirely — on such networks, use the manual Server IP field; if that also fails, the network itself is blocking the connection.

## Build Installers

The Tauri bundle step is enabled for both desktop apps. Build each app from its package folder:

```bash
cd cross_platform/apps/server
npm run build:mac-app
```

```bash
cd cross_platform/apps/client
npm run build:mac-app
```

Build outputs are written under:

```text
cross_platform/target/release/
cross_platform/target/release/bundle/
```

For full installer packages, use `npm run build` on the target platform.

Expected user-facing packages by platform:

- macOS: `.app` and `.dmg`
- Windows: `.msi` or `.exe`
- Linux: `.deb`, `.rpm`, or `.AppImage`

For release distribution, build on each target OS or use CI runners for each platform. A macOS machine should build the macOS package, a Windows runner should build the Windows package, and a Linux runner should build the Linux package.

This repo includes a GitHub Actions workflow at `.github/workflows/desktop-builds.yml`. Every branch push builds both apps on macOS, Windows, and Linux, then uploads downloadable workflow artifacts. Pushing a tag like `v0.1.0` also creates or updates a GitHub Release and uploads the packaged app assets to that release.

Before sharing with real users, add platform signing/notarization:

- macOS: Developer ID signing and notarization.
- Windows: code-sign the installer/executable.
- Linux: publish checksums and package metadata.

## Stress Test

Start the server app and create a room first. Then run synthetic clients against
the room, passing its class code with `--code` — the port is derived from the
code, so `--port` is only needed to override it:

```bash
cd cross_platform
cargo run -p stress-clients -- --host 127.0.0.1 --code ABCD --clients 20 --fps 5 --seconds 60
```

The tool sends the same protocol as the real client: one identity packet followed by JPEG frame packets.

## App Icon

The shared app icon source is `assets/exam-guard-icon.png`. Generated app icon assets live in each Tauri app's `src-tauri/icons` directory.

Regenerate icon assets with:

```bash
python3 cross_platform/tools/generate_app_icons.py
```
