use crate::capture::capture_primary_screen_jpeg;
use crate::discovery::{discovery_targets, DISCOVER_MESSAGE, SERVER_MESSAGE};
use crate::protocol::{identity_payload, write_packet, PacketType};
use serde::Serialize;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Clone, Debug, Serialize)]
pub struct ClientSnapshot {
    pub is_running: bool,
    pub is_connected: bool,
    pub status: String,
}

pub struct ClientRuntime {
    running: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    status: Arc<Mutex<String>>,
    worker: Option<JoinHandle<()>>,
}

impl ClientRuntime {
    pub fn start(
        student_name: String,
        student_id: String,
        port: u16,
        server_ip: Option<IpAddr>,
        join_code: String,
    ) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let connected = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(String::from("Looking for the server...")));

        let worker = {
            let running = Arc::clone(&running);
            let connected = Arc::clone(&connected);
            let status = Arc::clone(&status);

            thread::spawn(move || {
                while running.load(Ordering::SeqCst) {
                    connected.store(false, Ordering::SeqCst);
                    set_status(&status, "Looking for the server...");

                    // A manually entered server IP skips network discovery.
                    let discovered = match server_ip {
                        Some(ip) => Some(ip),
                        None => discover_server(port, &running),
                    };

                    match discovered {
                        Some(server_ip) => {
                            let addr = SocketAddr::new(server_ip, port);
                            set_status(&status, format!("Connecting to {addr}..."));

                            match TcpStream::connect_timeout(&addr, Duration::from_secs(4)) {
                                Ok(mut stream) => {
                                    let _ = stream.set_nodelay(true);
                                    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
                                    eprintln!("CLIENT: connected to {addr}");
                                    set_status(&status, "Connected, sending identity...");

                                    let identity =
                                        identity_payload(&join_code, &student_name, &student_id);
                                    if let Err(error) =
                                        write_packet(&mut stream, PacketType::Name, &identity)
                                    {
                                        connected.store(false, Ordering::SeqCst);
                                        set_status(
                                            &status,
                                            format!("Failed to send identity: {error}"),
                                        );
                                        eprintln!("CLIENT: failed to send identity: {error}");
                                        thread::sleep(Duration::from_secs(1));
                                        continue;
                                    }

                                    set_status(
                                        &status,
                                        "Connected, waiting for first screen frame...",
                                    );
                                    let stream_result = catch_unwind(AssertUnwindSafe(|| {
                                        stream_frames(&mut stream, &running, &connected, &status)
                                    }));

                                    connected.store(false, Ordering::SeqCst);

                                    if running.load(Ordering::SeqCst) {
                                        match stream_result {
                                            Ok(()) => {
                                                set_status(&status, "Disconnected from server");
                                                eprintln!("CLIENT: stream ended");
                                            }
                                            Err(_) => {
                                                set_status(
                                                    &status,
                                                    "Screen sharing worker crashed",
                                                );
                                                eprintln!("CLIENT: screen sharing worker panicked");
                                            }
                                        }
                                    }
                                }
                                Err(error) => {
                                    connected.store(false, Ordering::SeqCst);
                                    set_status(&status, format!("Connection failed: {error}"));
                                    eprintln!("CLIENT: connection to {addr} failed: {error}");
                                    thread::sleep(Duration::from_secs(1));
                                }
                            }
                        }
                        None => {
                            // Only happens on stop or socket failure; the
                            // sleep just guards against a bind-error spin.
                            thread::sleep(Duration::from_secs(1));
                        }
                    }
                }

                connected.store(false, Ordering::SeqCst);
                set_status(&status, "Stopped");
            })
        };

        Self {
            running,
            connected,
            status,
            worker: Some(worker),
        }
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        self.connected.store(false, Ordering::SeqCst);
        set_status(&self.status, "Stopped");

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    pub fn snapshot(&self) -> ClientSnapshot {
        ClientSnapshot {
            is_running: self.running.load(Ordering::SeqCst),
            is_connected: self.connected.load(Ordering::SeqCst),
            status: self
                .status
                .lock()
                .map(|value| value.clone())
                .unwrap_or_else(|_| String::from("Status unavailable")),
        }
    }
}

impl Drop for ClientRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

fn stream_frames(
    stream: &mut TcpStream,
    running: &Arc<AtomicBool>,
    connected: &Arc<AtomicBool>,
    status: &Arc<Mutex<String>>,
) {
    let mut sent_frames = 0_u64;

    while running.load(Ordering::SeqCst) {
        // 1280px keeps the teacher's zoomed view sharp; 2fps matches what
        // the dashboard actually displays (it polls at 1fps), so the wider
        // frames don't cost extra bandwidth versus the old 720px at 5fps.
        match capture_primary_screen_jpeg(60, 1280) {
            Ok(frame) => {
                if let Err(error) = write_packet(stream, PacketType::Picture, &frame) {
                    connected.store(false, Ordering::SeqCst);
                    set_status(status, format!("Disconnected from server: {error}"));
                    eprintln!("CLIENT: failed to send screen frame: {error}");
                    break;
                }

                sent_frames += 1;
                if !connected.swap(true, Ordering::SeqCst) {
                    set_status(status, "Screen sharing is active");
                    eprintln!("CLIENT: first screen frame sent ({} bytes)", frame.len());
                } else if sent_frames % 20 == 0 {
                    eprintln!(
                        "CLIENT: sent {sent_frames} screen frames, latest {} bytes",
                        frame.len()
                    );
                }
            }
            Err(error) => {
                set_status(status, format!("Screen capture failed: {error}"));
                eprintln!("CLIENT: screen capture failed: {error}");
            }
        }

        thread::sleep(Duration::from_millis(500));
    }
}

fn discover_server(port: u16, running: &Arc<AtomicBool>) -> Option<IpAddr> {
    // Bind to the room port so we can hear the server's broadcast beacons.
    // Fall back to an ephemeral port (probe replies still reach us there)
    // when the room port is taken, e.g. by a server on the same machine.
    let socket = match UdpSocket::bind(("0.0.0.0", port)) {
        Ok(socket) => socket,
        Err(error) => {
            eprintln!("CLIENT: UDP bind on port {port} failed ({error}), using ephemeral port");
            match UdpSocket::bind(("0.0.0.0", 0)) {
                Ok(socket) => socket,
                Err(error) => {
                    eprintln!("CLIENT: failed to bind UDP discovery socket: {error}");
                    return None;
                }
            }
        }
    };

    if let Err(error) = socket.set_broadcast(true) {
        eprintln!("CLIENT: failed to enable UDP broadcast: {error}");
    }
    if let Err(error) = socket.set_read_timeout(Some(Duration::from_secs(1))) {
        eprintln!("CLIENT: failed to set UDP read timeout: {error}");
    }

    let targets = discovery_targets(port);
    let localhost = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let mut probe_error_logged = false;
    let mut buffer = [0_u8; 64];

    // Probe roughly once per second until the server appears or the client
    // stops. Giving up and sleeping between search rounds (the old design)
    // left deaf gaps where a freshly started room went unnoticed for up to
    // 8 seconds; continuous listening caps connect latency at ~1 second.
    while running.load(Ordering::SeqCst) {
        // Server on this machine? A refused connect returns instantly, so
        // this stays cheap when there isn't one.
        if TcpStream::connect_timeout(&localhost, Duration::from_millis(300)).is_ok() {
            return Some(localhost.ip());
        }

        for target in &targets {
            if let Err(error) = socket.send_to(DISCOVER_MESSAGE, target) {
                if !probe_error_logged {
                    eprintln!("CLIENT: discovery probe to {target} failed: {error}");
                    probe_error_logged = true;
                }
            }
        }

        match socket.recv_from(&mut buffer) {
            Ok((count, source)) if &buffer[..count] == SERVER_MESSAGE => {
                eprintln!("CLIENT: discovered server at {source}");
                return Some(source.ip());
            }
            Ok(_) => {}
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.kind() == io::ErrorKind::TimedOut => {}
            // Windows surfaces ICMP port-unreachable replies to earlier
            // probes as ConnectionReset on the next recv; keep listening.
            Err(error) => {
                if !probe_error_logged {
                    eprintln!("CLIENT: discovery receive failed: {error}");
                    probe_error_logged = true;
                }
                thread::sleep(Duration::from_millis(300));
            }
        }
    }

    None
}

fn set_status(status: &Arc<Mutex<String>>, value: impl Into<String>) {
    if let Ok(mut current) = status.lock() {
        *current = value.into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::ServerRuntime;

    #[test]
    fn discovery_finds_server_started_after_search_began() {
        let port = 42_357;
        let running = Arc::new(AtomicBool::new(true));

        // Watchdog so a regression fails the test instead of hanging it.
        {
            let running = Arc::clone(&running);
            thread::spawn(move || {
                thread::sleep(Duration::from_secs(10));
                running.store(false, Ordering::SeqCst);
            });
        }

        let searcher = {
            let running = Arc::clone(&running);
            thread::spawn(move || discover_server(port, &running))
        };

        // The student is already searching; the teacher starts the room late.
        thread::sleep(Duration::from_millis(1_500));
        let mut runtime = ServerRuntime::start(
            String::from("Exam"),
            String::from("Course"),
            port.to_string(),
            port,
            None,
            String::new(),
        );

        let found = searcher.join().unwrap();
        runtime.stop();
        running.store(false, Ordering::SeqCst);

        assert!(found.is_some(), "discovery never noticed the late-started server");
    }
}
