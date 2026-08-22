use crate::discovery::{discovery_targets, DISCOVER_MESSAGE, SERVER_MESSAGE};
use crate::logging::SessionLogger;
use crate::protocol::{
    parse_identity, read_packet, write_packet, PacketType, CLIENT_NO_SCREEN_PERMISSION,
    DEVICE_ID_PREFIX, REJECT_WRONG_CODE,
};
use base64::{engine::general_purpose, Engine};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Drop a connection that sends nothing for this long (frames arrive every
/// 200ms, so this also reaps clients whose laptop slept or lost Wi-Fi
/// without a TCP reset).
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
/// Cap concurrent connections so a flood cannot exhaust threads/memory.
const MAX_CONNECTIONS: usize = 64;
/// How often the evidence logger saves each connected student's latest frame.
const EVIDENCE_INTERVAL: Duration = Duration::from_secs(60);

type Connections = Arc<Mutex<HashMap<u64, TcpStream>>>;
type Diagnostics = Arc<Mutex<RoomDiagnostics>>;

/// A device is treated as "still trying" for this long after its last probe.
const PROBE_MEMORY: Duration = Duration::from_secs(60);

/// Why a student might be missing from the grid. Without this the teacher
/// cannot tell a wrong code from a blocked firewall from an empty room —
/// all three look identical (nothing).
#[derive(Default)]
struct RoomDiagnostics {
    /// Connections dropped for presenting the wrong class code.
    wrong_code: u64,
    /// Devices that asked where the room is, and when they last asked.
    probed: HashMap<IpAddr, Instant>,
    /// Devices that got as far as a TCP connection.
    connected_peers: HashSet<IpAddr>,
}

impl RoomDiagnostics {
    /// Devices that found the room over UDP but never completed a TCP
    /// connection — the signature of a firewall or blocked port.
    fn unreachable(&self) -> usize {
        self.probed
            .iter()
            .filter(|(ip, at)| {
                at.elapsed() < PROBE_MEMORY && !self.connected_peers.contains(ip)
            })
            .count()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct StudentSnapshot {
    pub id: u64,
    pub name: String,
    pub student_id: String,
    pub image_data_url: Option<String>,
    pub connected_at_ms: u128,
    pub last_update_ms: u128,
    /// False once the socket drops — the tile becomes a tombstone the proctor
    /// dismisses, instead of the student silently vanishing from the grid.
    pub connected: bool,
    pub disconnected_at_ms: Option<u128>,
    /// The student's OS refused screen capture, so their frames show a bare
    /// desktop. Without this flag they look perfectly monitored.
    pub capture_blocked: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ServerSnapshot {
    pub is_running: bool,
    pub exam_name: String,
    pub course_name: String,
    pub room_number: String,
    pub students: Vec<StudentSnapshot>,
    /// Folder where this session's evidence log is written, if logging is on.
    pub log_dir: Option<String>,
    /// The room's join code, shown to the teacher to read out to students.
    pub join_code: String,
    /// Port derived from the join code. Surfaced only so the dashboard can
    /// show it for students still on an older client that asks for a number.
    pub port: u16,
    /// Connections rejected for the wrong class code — tells the teacher to
    /// re-read the code rather than wonder why nobody is joining.
    pub wrong_code_attempts: u64,
    /// Devices that found the room but could not connect (firewall/blocked
    /// port). Distinguishes "blocked" from "nobody has opened the app yet".
    pub unreachable_devices: usize,
}

pub struct ServerRuntime {
    running: Arc<AtomicBool>,
    students: Arc<Mutex<Vec<StudentSnapshot>>>,
    connections: Connections,
    exam_name: String,
    course_name: String,
    room_number: String,
    join_code: String,
    port: u16,
    log_dir: Option<String>,
    session_dir: Option<PathBuf>,
    clear_on_finish: Arc<AtomicBool>,
    diagnostics: Diagnostics,
    workers: Vec<JoinHandle<()>>,
}

impl ServerRuntime {
    pub fn start(
        exam_name: String,
        course_name: String,
        room_number: String,
        port: u16,
        log_base_dir: Option<PathBuf>,
        join_code: String,
    ) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let students = Arc::new(Mutex::new(Vec::new()));
        let connections: Connections = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics: Diagnostics = Arc::new(Mutex::new(RoomDiagnostics::default()));

        // Best-effort evidence logging; a failure here must not stop the exam.
        let logger = log_base_dir.and_then(|base| {
            match SessionLogger::new(&base, &room_number) {
                Ok(logger) => Some(Arc::new(logger)),
                Err(error) => {
                    eprintln!("SERVER: session logging disabled ({error})");
                    None
                }
            }
        });
        let session_dir = logger.as_ref().map(|logger| logger.dir().to_path_buf());
        let log_dir = session_dir
            .as_ref()
            .map(|dir| dir.display().to_string());

        let accept_worker = {
            let running = Arc::clone(&running);
            let students = Arc::clone(&students);
            let connections = Arc::clone(&connections);
            let logger = logger.clone();
            let join_code = join_code.clone();
            let diagnostics = Arc::clone(&diagnostics);

            thread::spawn(move || {
                run_tcp_listener(
                    port,
                    running,
                    students,
                    connections,
                    logger,
                    join_code,
                    diagnostics,
                )
            })
        };

        let broadcast_worker = {
            let running = Arc::clone(&running);
            let diagnostics = Arc::clone(&diagnostics);
            thread::spawn(move || run_udp_discovery(port, running, diagnostics))
        };

        let mut workers = vec![accept_worker, broadcast_worker];

        if let Some(logger) = logger {
            let running = Arc::clone(&running);
            let students = Arc::clone(&students);
            workers.push(thread::spawn(move || {
                run_evidence_logger(running, students, logger)
            }));
        }

        Self {
            running,
            students,
            connections,
            exam_name,
            course_name,
            room_number,
            join_code,
            port,
            log_dir,
            session_dir,
            clear_on_finish: Arc::new(AtomicBool::new(false)),
            diagnostics,
            workers,
        }
    }

    /// Remove a dismissed tombstone (or any student) by id.
    pub fn dismiss_student(&self, id: u64) {
        if let Ok(mut students) = self.students.lock() {
            students.retain(|student| student.id != id);
        }
    }

    /// Opt in/out of deleting this session's evidence folder when the room
    /// finishes (Stop pressed or app quit). Off by default.
    pub fn set_clear_evidence(&self, enabled: bool) {
        self.clear_on_finish.store(enabled, Ordering::SeqCst);
    }

    /// Delete this session's evidence folder if the teacher opted in. Safe to
    /// call more than once (missing dir is ignored).
    ///
    /// Retries briefly: a just-disconnected student's handler thread may still
    /// be flushing its final `events.csv` write, and on Windows an open file
    /// handle blocks directory deletion until that thread finishes.
    /// ponytail: bounded retry, not full handler-thread join-tracking.
    pub fn clear_evidence(&self) {
        if !self.clear_on_finish.load(Ordering::SeqCst) {
            return;
        }
        let Some(dir) = &self.session_dir else { return };

        for attempt in 0..10 {
            match std::fs::remove_dir_all(dir) {
                Ok(()) => {
                    eprintln!("SERVER: cleared evidence folder {}", dir.display());
                    return;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(error) if attempt == 9 => {
                    eprintln!("SERVER: failed to clear evidence: {error}");
                    return;
                }
                Err(_) => thread::sleep(Duration::from_millis(100)),
            }
        }
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);

        // Shut down every student socket so their handler threads unblock
        // and students see a clean disconnect instead of a silent orphan.
        if let Ok(connections) = self.connections.lock() {
            for stream in connections.values() {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }

        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }

        if let Ok(mut students) = self.students.lock() {
            students.clear();
        }

        // Honour the teacher's opt-in to wipe evidence when the room finishes.
        self.clear_evidence();
    }

    pub fn snapshot(&self) -> ServerSnapshot {
        ServerSnapshot {
            is_running: self.running.load(Ordering::SeqCst),
            exam_name: self.exam_name.clone(),
            course_name: self.course_name.clone(),
            room_number: self.room_number.clone(),
            students: self
                .students
                .lock()
                .map(|students| students.clone())
                .unwrap_or_default(),
            log_dir: self.log_dir.clone(),
            join_code: self.join_code.clone(),
            port: self.port,
            wrong_code_attempts: self
                .diagnostics
                .lock()
                .map(|d| d.wrong_code)
                .unwrap_or(0),
            unreachable_devices: self
                .diagnostics
                .lock()
                .map(|d| d.unreachable())
                .unwrap_or(0),
        }
    }
}

/// Generate a 4-character room join code from an unambiguous alphabet
/// (no 0/O/1/I). Time-seeded — unpredictable enough to stop a bystander who
/// only overheard the room number from joining, which is all it needs to do.
pub fn generate_join_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(1) as u64
        | 1;
    let mut code = String::with_capacity(4);
    for _ in 0..4 {
        // xorshift64 step for a cheap, dependency-free spread.
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        code.push(ALPHABET[(seed % ALPHABET.len() as u64) as usize] as char);
    }
    code
}

/// Map a class code to the port its room listens on, so students only ever
/// need the code — the port is derived, not typed.
///
/// Deterministic FNV-1a. The Swift apps duplicate this exactly; the test
/// vectors below are asserted in both languages to keep them in lockstep.
pub fn port_for_code(code: &str) -> u16 {
    let mut hash: u32 = 2_166_136_261;
    for byte in code.trim().to_uppercase().bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    // 20000..=44999: above privileged ports, below the ephemeral range.
    20_000 + (hash % 25_000) as u16
}

impl Drop for ServerRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_tcp_listener(
    port: u16,
    running: Arc<AtomicBool>,
    students: Arc<Mutex<Vec<StudentSnapshot>>>,
    connections: Connections,
    logger: Option<Arc<SessionLogger>>,
    join_code: String,
    diagnostics: Diagnostics,
) {
    let join_code = Arc::new(join_code);
    let listener = match TcpListener::bind(("0.0.0.0", port)) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("SERVER: failed to bind TCP listener on {port}: {error}");
            running.store(false, Ordering::SeqCst);
            return;
        }
    };

    let _ = listener.set_nonblocking(true);
    let mut next_id = 1_u64;

    while running.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, peer_addr)) => {
                let at_capacity = connections
                    .lock()
                    .map(|map| map.len() >= MAX_CONNECTIONS)
                    .unwrap_or(true);
                if at_capacity {
                    eprintln!("SERVER: rejecting connection from {peer_addr}: at capacity");
                    continue;
                }

                let id = next_id;
                next_id += 1;
                eprintln!("SERVER: accepted TCP connection {id} from {peer_addr}");
                // Reaching TCP means this device is not firewall-blocked.
                if let Ok(mut diag) = diagnostics.lock() {
                    diag.connected_peers.insert(peer_addr.ip());
                }

                if let Err(error) = stream.set_nonblocking(false) {
                    eprintln!("SERVER: failed to set connection {id} blocking mode: {error}");
                    continue;
                }
                // Idle sockets get reaped instead of blocking a thread forever.
                let _ = stream.set_read_timeout(Some(IDLE_TIMEOUT));

                if let (Ok(clone), Ok(mut map)) = (stream.try_clone(), connections.lock()) {
                    map.insert(id, clone);
                }

                let running = Arc::clone(&running);
                let students = Arc::clone(&students);
                let connections = Arc::clone(&connections);
                let logger = logger.clone();
                let join_code = Arc::clone(&join_code);
                let diagnostics = Arc::clone(&diagnostics);
                thread::spawn(move || {
                    handle_student(
                        id,
                        stream,
                        running,
                        students,
                        connections,
                        logger,
                        join_code,
                        diagnostics,
                    )
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => {
                eprintln!("SERVER: accept failed: {error}");
                thread::sleep(Duration::from_millis(300));
            }
        }
    }
}

fn run_udp_discovery(port: u16, running: Arc<AtomicBool>, diagnostics: Diagnostics) {
    // Bind to the room port so client "discover" probes reach us. Fall back to
    // an ephemeral port (beacon-only mode) if the room port is taken.
    let socket = match UdpSocket::bind(("0.0.0.0", port)) {
        Ok(socket) => socket,
        Err(error) => {
            eprintln!(
                "SERVER: failed to bind UDP discovery socket on port {port}: {error}; \
                 falling back to beacon-only discovery"
            );
            match UdpSocket::bind(("0.0.0.0", 0)) {
                Ok(socket) => socket,
                Err(error) => {
                    eprintln!("SERVER: failed to bind UDP beacon socket: {error}");
                    return;
                }
            }
        }
    };

    if let Err(error) = socket.set_broadcast(true) {
        eprintln!("SERVER: failed to enable UDP broadcast: {error}");
    }
    if let Err(error) = socket.set_read_timeout(Some(Duration::from_millis(500))) {
        eprintln!("SERVER: failed to set UDP read timeout: {error}");
    }

    let mut targets = discovery_targets(port);
    // Beacon to loopback as well. A broadcast is not reliably delivered back
    // to a socket on the same host, so without this a client that grabbed the
    // room port before us (server and client on one machine) never hears us.
    targets.push(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port));
    eprintln!("SERVER: announcing room {port} to {targets:?}");

    let mut buffer = [0_u8; 64];
    let mut last_beacon: Option<Instant> = None;
    let mut beacon_error_logged = false;

    while running.load(Ordering::SeqCst) {
        let beacon_due = last_beacon
            .map(|at| at.elapsed() >= Duration::from_secs(2))
            .unwrap_or(true);

        if beacon_due {
            for target in &targets {
                if let Err(error) = socket.send_to(SERVER_MESSAGE, target) {
                    if !beacon_error_logged {
                        eprintln!("SERVER: beacon to {target} failed: {error}");
                        beacon_error_logged = true;
                    }
                }
            }
            last_beacon = Some(Instant::now());
        }

        match socket.recv_from(&mut buffer) {
            Ok((count, source)) if &buffer[..count] == DISCOVER_MESSAGE => {
                eprintln!("SERVER: discovery probe from {source}");
                // Remember who is looking; if they never reach TCP, something
                // between us is blocking the connection.
                if let Ok(mut diag) = diagnostics.lock() {
                    diag.probed.insert(source.ip(), Instant::now());
                }
                if let Err(error) = socket.send_to(SERVER_MESSAGE, source) {
                    eprintln!("SERVER: failed to answer probe from {source}: {error}");
                }
            }
            Ok(_) => {}
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(error) => {
                eprintln!("SERVER: UDP discovery receive failed: {error}");
                thread::sleep(Duration::from_millis(300));
            }
        }
    }
}

fn handle_student(
    id: u64,
    mut stream: TcpStream,
    running: Arc<AtomicBool>,
    students: Arc<Mutex<Vec<StudentSnapshot>>>,
    connections: Connections,
    logger: Option<Arc<SessionLogger>>,
    join_code: Arc<String>,
    diagnostics: Diagnostics,
) {
    // When a room has a join code, a connection must present it before it can
    // register — this is what stops a bystander who only knows the room number
    // from appearing as a (possibly spoofed) student.
    let require_code = !join_code.is_empty();
    let mut is_registered = false;
    let mut received_frames = 0_u64;
    let mut identity = (String::from("Unknown"), String::new());

    while running.load(Ordering::SeqCst) {
        let packet = match read_packet(&mut stream) {
            Ok(packet) => packet,
            Err(error) => {
                if is_registered {
                    eprintln!("SERVER: student connection {id} closed: {error}");
                } else {
                    eprintln!("SERVER: unregistered connection {id} closed: {error}");
                }
                break;
            }
        };

        match packet.packet_type {
            PacketType::Name => {
                let (code, name, student_id) = parse_identity(&packet.payload);
                if require_code && code != *join_code {
                    eprintln!("SERVER: rejecting connection {id}: wrong join code");
                    if let Ok(mut diag) = diagnostics.lock() {
                        diag.wrong_code = diag.wrong_code.saturating_add(1);
                    }
                    // Tell the student *why* before hanging up, so they see
                    // "wrong class code" instead of a generic socket error.
                    let _ = write_packet(
                        &mut stream,
                        PacketType::Message,
                        REJECT_WRONG_CODE.as_bytes(),
                    );
                    break;
                }
                // A reconnecting student replaces their own tombstone rather
                // than showing twice. Clients need not send a student ID, so
                // fall back to the name as the identity key.
                let identity_key = identity_key(&name, &student_id);
                if !identity_key.is_empty() {
                    drop_tombstones(&students, &identity_key, id);
                }
                identity = (name.clone(), student_id.clone());
                upsert_student(&students, id, |student| {
                    // Keep the "Unknown" seed rather than blanking the tile if
                    // a client sends no name at all.
                    if !name.is_empty() {
                        student.name = name;
                    }
                    student.student_id = student_id;
                    student.last_update_ms = now_ms();
                });
                if !is_registered {
                    if let Some(logger) = &logger {
                        logger.event("connect", &identity.0, &identity.1);
                    }
                }
                is_registered = true;
                eprintln!("SERVER: registered student connection {id}");
            }
            PacketType::Picture => {
                if !is_registered {
                    // A framed room requires the join code first; an unverified
                    // frame-before-identity connection is dropped.
                    if require_code {
                        eprintln!("SERVER: rejecting connection {id}: frame before join code");
                        break;
                    }
                    upsert_student(&students, id, |student| {
                        student.last_update_ms = now_ms();
                    });
                    is_registered = true;
                    eprintln!("SERVER: connection {id} sent a frame before identity");
                }

                received_frames += 1;
                let encoded = general_purpose::STANDARD.encode(packet.payload);
                let image_data_url = format!("data:image/jpeg;base64,{encoded}");
                update_student(&students, id, |student| {
                    student.image_data_url = Some(image_data_url);
                    student.last_update_ms = now_ms();
                });

                if received_frames == 1 || received_frames % 20 == 0 {
                    eprintln!("SERVER: received {received_frames} frame(s) from connection {id}");
                }
            }
            PacketType::Message => {
                if packet.payload == CLIENT_NO_SCREEN_PERMISSION.as_bytes() {
                    eprintln!("SERVER: connection {id} reports blocked screen capture");
                    update_student(&students, id, |student| {
                        student.capture_blocked = true;
                    });
                }
            }
        }
    }

    if is_registered {
        // Keep the tile as a tombstone so a dropped student is visible to the
        // proctor instead of silently vanishing.
        update_student(&students, id, |student| {
            student.connected = false;
            student.disconnected_at_ms = Some(now_ms());
        });
        if let Some(logger) = &logger {
            logger.event("disconnect", &identity.0, &identity.1);
        }
    } else if let Ok(mut students) = students.lock() {
        // Never-registered connection (scanner / failed handshake): just drop it.
        students.retain(|student| student.id != id);
    }
    // Deregister so the fd is released and stop() doesn't touch dead sockets.
    if let Ok(mut connections) = connections.lock() {
        connections.remove(&id);
    }
}

/// How a student is recognised across reconnects: their ID when they send one,
/// otherwise their name. Clients are not required to collect a student ID.
fn identity_key(name: &str, student_id: &str) -> String {
    if student_id.is_empty() {
        name.trim().to_string()
    } else {
        student_id.to_string()
    }
}

/// Filename-friendly label: a real student ID if the client sent one, else
/// the student's name. Generated device ids are for matching, not reading.
fn evidence_label(name: &str, student_id: &str) -> String {
    if student_id.is_empty() || student_id.starts_with(DEVICE_ID_PREFIX) {
        name.trim().to_string()
    } else {
        student_id.to_string()
    }
}

/// Remove offline tombstones matching `key` other than the live connection
/// `keep_id`, so a reconnecting student collapses back to a single tile.
fn drop_tombstones(students: &Arc<Mutex<Vec<StudentSnapshot>>>, key: &str, keep_id: u64) {
    if let Ok(mut students) = students.lock() {
        students.retain(|student| {
            student.id == keep_id
                || student.connected
                || identity_key(&student.name, &student.student_id) != key
        });
    }
}

fn update_student(
    students: &Arc<Mutex<Vec<StudentSnapshot>>>,
    id: u64,
    update: impl FnOnce(&mut StudentSnapshot),
) {
    if let Ok(mut students) = students.lock() {
        if let Some(student) = students.iter_mut().find(|student| student.id == id) {
            update(student);
        }
    }
}

fn upsert_student(
    students: &Arc<Mutex<Vec<StudentSnapshot>>>,
    id: u64,
    update: impl FnOnce(&mut StudentSnapshot),
) {
    if let Ok(mut students) = students.lock() {
        if students.iter().all(|student| student.id != id) {
            let now = now_ms();
            students.push(StudentSnapshot {
                id,
                name: String::from("Unknown"),
                student_id: String::new(),
                image_data_url: None,
                connected_at_ms: now,
                last_update_ms: now,
                connected: true,
                disconnected_at_ms: None,
                capture_blocked: false,
            });
        }

        if let Some(student) = students.iter_mut().find(|student| student.id == id) {
            update(student);
        }
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

/// Periodically snapshot each connected student's latest frame to disk so the
/// proctor has evidence after the exam. Polls the shutdown flag frequently so
/// stop() returns quickly.
fn run_evidence_logger(
    running: Arc<AtomicBool>,
    students: Arc<Mutex<Vec<StudentSnapshot>>>,
    logger: Arc<SessionLogger>,
) {
    let mut last_save: Option<Instant> = None;

    while running.load(Ordering::SeqCst) {
        let due = last_save
            .map(|at| at.elapsed() >= EVIDENCE_INTERVAL)
            .unwrap_or(true);

        if due {
            let frames: Vec<(String, String)> = students
                .lock()
                .map(|students| {
                    students
                        .iter()
                        .filter(|student| student.connected)
                        .filter_map(|student| {
                            // Prefer a stable, human-meaningful filename. A
                            // generated device id dedups well but reads
                            // terribly, so the name wins for filenames.
                            let label = match evidence_label(&student.name, &student.student_id) {
                                label if !label.is_empty() => label,
                                _ => format!("id{}", student.id),
                            };
                            student
                                .image_data_url
                                .clone()
                                .map(|data_url| (label, data_url))
                        })
                        .collect()
                })
                .unwrap_or_default();

            for (label, data_url) in frames {
                if let Some(jpeg) = decode_data_url(&data_url) {
                    logger.save_frame(&label, &jpeg);
                }
            }
            last_save = Some(Instant::now());
        }

        thread::sleep(Duration::from_millis(500));
    }
}

/// Strip the `data:image/jpeg;base64,` prefix and decode back to JPEG bytes.
fn decode_data_url(data_url: &str) -> Option<Vec<u8>> {
    let base64 = data_url.split(',').nth(1)?;
    general_purpose::STANDARD.decode(base64).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{identity_payload, write_packet};
    use std::net::TcpStream;

    #[test]
    fn port_for_code_is_deterministic_and_in_range() {
        for code in ["N7KU", "MATH", "AAAA", "ZZ99", "7GK4"] {
            let port = port_for_code(code);
            assert_eq!(port, port_for_code(code), "must be stable");
            assert!((20_000..=44_999).contains(&port), "{code} -> {port} out of range");
        }
    }

    #[test]
    fn port_for_code_ignores_case_and_padding() {
        assert_eq!(port_for_code("math"), port_for_code("MATH"));
        assert_eq!(port_for_code("  MATH \n"), port_for_code("MATH"));
    }

    #[test]
    fn port_for_code_separates_distinct_codes() {
        let codes = ["N7KU", "MATH", "AAAA", "ZZ99", "7GK4", "ABCD"];
        let mut ports: Vec<u16> = codes.iter().map(|c| port_for_code(c)).collect();
        ports.sort_unstable();
        ports.dedup();
        assert_eq!(ports.len(), codes.len(), "codes collided on a port");
    }

    /// Cross-language contract: the Swift apps derive ports with the same
    /// algorithm and assert these exact numbers. Changing them breaks
    /// Swift<->Tauri interoperability, so treat a failure here as a protocol
    /// change, not a test to update.
    #[test]
    fn port_vectors_match_the_swift_implementation() {
        assert_eq!(port_for_code("N7KU"), 38_242);
        assert_eq!(port_for_code("MATH"), 34_703);
        assert_eq!(port_for_code("AAAA"), 37_697);
    }

    /// End-to-end shape of the v0.2.0 flow: the server binds the port derived
    /// from its class code, and a client that knows only the code derives the
    /// same port and registers. This also covers an older client that types
    /// the port and code by hand — on the wire the two are identical.
    #[test]
    fn class_code_alone_reaches_the_room() {
        let code = "TEST";
        let port = port_for_code(code);
        let runtime = ServerRuntime::start(
            String::from("Exam"),
            String::from("Course"),
            String::from(code),
            port,
            None,
            String::from(code),
        );

        thread::sleep(Duration::from_millis(200));

        // The client side knows only the code.
        let mut stream = TcpStream::connect(("127.0.0.1", port_for_code(code))).unwrap();
        write_packet(
            &mut stream,
            PacketType::Name,
            &identity_payload(code, "Ada", "STU-1"),
        )
        .unwrap();
        thread::sleep(Duration::from_millis(200));

        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.port, port, "snapshot must expose the derived port");
        assert_eq!(snapshot.join_code, code);
        assert_eq!(snapshot.students.len(), 1, "client should have registered");
        assert_eq!(snapshot.students[0].name, "Ada");
    }

    #[test]
    fn identity_key_falls_back_to_name_without_a_student_id() {
        assert_eq!(identity_key("Ada", "STU-1"), "STU-1");
        assert_eq!(identity_key("Ada", ""), "Ada");
        assert_eq!(identity_key("  Ada  ", ""), "Ada");
        assert_eq!(identity_key("", ""), "");
    }

    /// Students are no longer asked for an ID, so reconnect dedup has to work
    /// off the name alone — otherwise a dropped student who rejoins shows up
    /// twice (live tile + their own tombstone).
    #[test]
    fn reconnect_without_student_id_replaces_tombstone() {
        let port = 42_366;
        let runtime = ServerRuntime::start(
            String::from("Exam"),
            String::from("Course"),
            port.to_string(),
            port,
            None,
            String::new(),
        );

        thread::sleep(Duration::from_millis(200));

        // Join with a name but no student ID, then drop.
        let mut first = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write_packet(&mut first, PacketType::Name, &identity_payload("", "Ada", "")).unwrap();
        thread::sleep(Duration::from_millis(150));
        drop(first);
        thread::sleep(Duration::from_millis(300));
        assert_eq!(runtime.snapshot().students.len(), 1, "tombstone expected");

        // Same student rejoins — should collapse back to one live tile.
        let mut second = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write_packet(&mut second, PacketType::Name, &identity_payload("", "Ada", "")).unwrap();
        thread::sleep(Duration::from_millis(200));

        let students = runtime.snapshot().students;
        assert_eq!(students.len(), 1, "reconnect must not duplicate the tile");
        assert!(students[0].connected);
        assert_eq!(students[0].name, "Ada");
    }

    #[test]
    fn empty_name_does_not_blank_the_tile() {
        let port = 42_368;
        let runtime = ServerRuntime::start(
            String::from("Exam"),
            String::from("Course"),
            port.to_string(),
            port,
            None,
            String::new(),
        );

        thread::sleep(Duration::from_millis(200));

        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write_packet(&mut stream, PacketType::Name, &identity_payload("", "", "")).unwrap();
        thread::sleep(Duration::from_millis(200));

        let students = runtime.snapshot().students;
        assert_eq!(students.len(), 1);
        assert_eq!(students[0].name, "Unknown", "must not render a blank tile");
    }

    /// A mistyped code used to produce a silent connect/disconnect loop with
    /// no explanation for the student and no signal at all for the teacher.
    #[test]
    fn wrong_code_is_explained_to_the_student_and_counted_for_the_teacher() {
        let port = 42_370;
        let runtime = ServerRuntime::start(
            String::from("Exam"),
            String::from("Course"),
            port.to_string(),
            port,
            None,
            String::from("7GK4"),
        );

        thread::sleep(Duration::from_millis(200));

        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write_packet(
            &mut stream,
            PacketType::Name,
            &identity_payload("XXXX", "Mallory", ""),
        )
        .unwrap();

        // The student's client can read the reason instead of guessing from a
        // broken pipe.
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let packet = read_packet(&mut stream).expect("server should explain the refusal");
        assert_eq!(packet.packet_type, PacketType::Message);
        assert_eq!(packet.payload, REJECT_WRONG_CODE.as_bytes());

        // And the teacher can see that someone is trying with a bad code.
        thread::sleep(Duration::from_millis(150));
        let snapshot = runtime.snapshot();
        assert!(snapshot.students.is_empty(), "must not register");
        assert_eq!(snapshot.wrong_code_attempts, 1);
    }

    #[test]
    fn evidence_label_prefers_a_readable_name_over_a_device_id() {
        assert_eq!(evidence_label("Ada", "STU-1"), "STU-1");
        assert_eq!(evidence_label("Ada", "dev:abc123"), "Ada");
        assert_eq!(evidence_label("Ada", ""), "Ada");
        // ...but dedup still keys on the device id, which is the whole point.
        assert_eq!(identity_key("Ada", "dev:abc123"), "dev:abc123");
    }

    #[test]
    fn stop_disconnects_connected_students() {
        let port = 42_355;
        let mut runtime = ServerRuntime::start(
            String::from("Exam"),
            String::from("Course"),
            port.to_string(),
            port,
            None,
            String::new(),
        );

        thread::sleep(Duration::from_millis(200));

        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write_packet(
            &mut stream,
            PacketType::Name,
            &identity_payload("", "Probe Student", "PROBE-2"),
        )
        .unwrap();
        thread::sleep(Duration::from_millis(200));

        runtime.stop();

        // The server must have shut our socket down: a read now returns
        // EOF/error instead of blocking on an orphaned connection.
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut buffer = [0_u8; 8];
        use std::io::Read;
        let result = stream.read(&mut buffer);
        assert!(matches!(result, Ok(0) | Err(_)), "socket still open: {result:?}");
    }

    #[test]
    fn disconnected_student_becomes_tombstone_then_dismissable() {
        let port = 42_358;
        let runtime = ServerRuntime::start(
            String::from("Exam"),
            String::from("Course"),
            port.to_string(),
            port,
            None,
            String::new(),
        );

        thread::sleep(Duration::from_millis(200));

        // Register, then drop the connection.
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write_packet(
            &mut stream,
            PacketType::Name,
            &identity_payload("", "Ada", "STU-1"),
        )
        .unwrap();
        thread::sleep(Duration::from_millis(150));
        drop(stream);
        thread::sleep(Duration::from_millis(300));

        // Student is kept as an offline tombstone, not removed.
        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.students.len(), 1, "tombstone should remain");
        let tombstone = &snapshot.students[0];
        assert!(!tombstone.connected);
        assert!(tombstone.disconnected_at_ms.is_some());
        let tombstone_id = tombstone.id;

        // Dismissing removes it.
        runtime.dismiss_student(tombstone_id);
        assert!(runtime.snapshot().students.is_empty(), "dismiss should clear tombstone");
    }

    #[test]
    fn reconnect_replaces_own_tombstone() {
        let port = 42_360;
        let runtime = ServerRuntime::start(
            String::from("Exam"),
            String::from("Course"),
            port.to_string(),
            port,
            None,
            String::new(),
        );

        thread::sleep(Duration::from_millis(200));

        // First connection registers and drops.
        let mut first = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write_packet(&mut first, PacketType::Name, &identity_payload("", "Ada", "STU-9")).unwrap();
        thread::sleep(Duration::from_millis(150));
        drop(first);
        thread::sleep(Duration::from_millis(300));
        assert_eq!(runtime.snapshot().students.len(), 1);

        // Same student reconnects — should collapse to one live tile.
        let mut second = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write_packet(&mut second, PacketType::Name, &identity_payload("", "Ada", "STU-9")).unwrap();
        thread::sleep(Duration::from_millis(200));

        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.students.len(), 1, "reconnect should not duplicate");
        assert!(snapshot.students[0].connected, "surviving tile should be live");
    }

    #[test]
    fn server_answers_udp_discovery_probe() {
        let port = 42_351;
        let mut runtime = ServerRuntime::start(
            String::from("Exam"),
            String::from("Course"),
            port.to_string(),
            port,
            None,
            String::new(),
        );

        thread::sleep(Duration::from_millis(200));

        let probe = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        probe
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        probe
            .send_to(DISCOVER_MESSAGE, ("127.0.0.1", port))
            .unwrap();

        let mut buffer = [0_u8; 64];
        let (count, source) = probe.recv_from(&mut buffer).unwrap();
        runtime.stop();

        assert_eq!(&buffer[..count], SERVER_MESSAGE);
        assert_eq!(source.port(), port);
    }

    #[test]
    fn server_keeps_registered_picture_connection_open() {
        let port = 42_349;
        let mut runtime = ServerRuntime::start(
            String::from("Exam"),
            String::from("Course"),
            port.to_string(),
            port,
            None,
            String::new(),
        );

        thread::sleep(Duration::from_millis(200));

        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write_packet(
            &mut stream,
            PacketType::Name,
            &identity_payload("", "Probe Student", "PROBE-1"),
        )
        .unwrap();

        for _ in 0..4 {
            write_packet(&mut stream, PacketType::Picture, b"picture-payload").unwrap();
            thread::sleep(Duration::from_millis(50));
        }

        let snapshot = runtime.snapshot();
        runtime.stop();

        assert_eq!(snapshot.students.len(), 1);
        assert_eq!(snapshot.students[0].name, "Probe Student");
        assert!(snapshot.students[0].image_data_url.is_some());
    }

    #[test]
    fn wrong_join_code_is_rejected_but_correct_one_registers() {
        let port = 42_362;
        let runtime = ServerRuntime::start(
            String::from("Exam"),
            String::from("Course"),
            port.to_string(),
            port,
            None,
            String::from("7GK4"),
        );

        thread::sleep(Duration::from_millis(200));

        // Wrong code: connection is dropped, never registered.
        let mut bad = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write_packet(&mut bad, PacketType::Name, &identity_payload("XXXX", "Mallory", "M-1")).unwrap();
        thread::sleep(Duration::from_millis(200));
        assert!(
            runtime.snapshot().students.is_empty(),
            "wrong code must not register"
        );

        // Correct code: registers normally.
        let mut good = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write_packet(&mut good, PacketType::Name, &identity_payload("7GK4", "Ada", "STU-1")).unwrap();
        thread::sleep(Duration::from_millis(200));
        let students = runtime.snapshot().students;
        assert_eq!(students.len(), 1);
        assert_eq!(students[0].name, "Ada");
    }

    #[test]
    fn clear_evidence_removes_session_folder_only_when_opted_in() {
        let base = std::env::temp_dir().join(format!("examguard_test_{}", now_ms()));
        std::fs::create_dir_all(&base).unwrap();

        // Off by default: folder survives stop().
        let mut kept = ServerRuntime::start(
            String::from("Exam"),
            String::from("Course"),
            String::from("42363"),
            42_363,
            Some(base.clone()),
            String::new(),
        );
        let kept_dir = kept.snapshot().log_dir.map(std::path::PathBuf::from).unwrap();
        assert!(kept_dir.exists());
        kept.stop();
        assert!(kept_dir.exists(), "must keep evidence when not opted in");

        // Opted in: folder deleted on stop().
        let mut cleared = ServerRuntime::start(
            String::from("Exam"),
            String::from("Course"),
            String::from("42364"),
            42_364,
            Some(base.clone()),
            String::new(),
        );
        let cleared_dir = cleared.snapshot().log_dir.map(std::path::PathBuf::from).unwrap();
        cleared.set_clear_evidence(true);
        cleared.stop();
        assert!(!cleared_dir.exists(), "must delete evidence when opted in");

        let _ = std::fs::remove_dir_all(&base);
    }
}
