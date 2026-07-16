use crate::discovery::{discovery_targets, DISCOVER_MESSAGE, SERVER_MESSAGE};
use crate::logging::SessionLogger;
use crate::protocol::{parse_identity, read_packet, PacketType};
use base64::{engine::general_purpose, Engine};
use serde::Serialize;
use std::collections::HashMap;
use std::net::{Shutdown, TcpListener, TcpStream, UdpSocket};
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
}

pub struct ServerRuntime {
    running: Arc<AtomicBool>,
    students: Arc<Mutex<Vec<StudentSnapshot>>>,
    connections: Connections,
    exam_name: String,
    course_name: String,
    room_number: String,
    log_dir: Option<String>,
    workers: Vec<JoinHandle<()>>,
}

impl ServerRuntime {
    pub fn start(
        exam_name: String,
        course_name: String,
        room_number: String,
        port: u16,
        log_base_dir: Option<PathBuf>,
    ) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let students = Arc::new(Mutex::new(Vec::new()));
        let connections: Connections = Arc::new(Mutex::new(HashMap::new()));

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
        let log_dir = logger
            .as_ref()
            .map(|logger| logger.dir().display().to_string());

        let accept_worker = {
            let running = Arc::clone(&running);
            let students = Arc::clone(&students);
            let connections = Arc::clone(&connections);
            let logger = logger.clone();

            thread::spawn(move || run_tcp_listener(port, running, students, connections, logger))
        };

        let broadcast_worker = {
            let running = Arc::clone(&running);
            thread::spawn(move || run_udp_discovery(port, running))
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
            log_dir,
            workers,
        }
    }

    /// Remove a dismissed tombstone (or any student) by id.
    pub fn dismiss_student(&self, id: u64) {
        if let Ok(mut students) = self.students.lock() {
            students.retain(|student| student.id != id);
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
        }
    }
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
) {
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
                thread::spawn(move || {
                    handle_student(id, stream, running, students, connections, logger)
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

fn run_udp_discovery(port: u16, running: Arc<AtomicBool>) {
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

    let targets = discovery_targets(port);
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
) {
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
                let (name, student_id) = parse_identity(&packet.payload);
                // A reconnecting student replaces their own tombstone rather
                // than showing twice.
                if !student_id.is_empty() {
                    drop_tombstones(&students, &student_id, id);
                }
                identity = (name.clone(), student_id.clone());
                upsert_student(&students, id, |student| {
                    student.name = name;
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
            PacketType::Message => {}
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

/// Remove offline tombstones for `student_id` other than the live connection
/// `keep_id`, so a reconnecting student collapses back to a single tile.
fn drop_tombstones(students: &Arc<Mutex<Vec<StudentSnapshot>>>, student_id: &str, keep_id: u64) {
    if let Ok(mut students) = students.lock() {
        students
            .retain(|student| student.id == keep_id || student.connected || student.student_id != student_id);
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
                            let label = if student.student_id.is_empty() {
                                format!("id{}", student.id)
                            } else {
                                student.student_id.clone()
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
    fn stop_disconnects_connected_students() {
        let port = 42_355;
        let mut runtime = ServerRuntime::start(
            String::from("Exam"),
            String::from("Course"),
            port.to_string(),
            port,
            None,
        );

        thread::sleep(Duration::from_millis(200));

        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write_packet(
            &mut stream,
            PacketType::Name,
            &identity_payload("Probe Student", "PROBE-2"),
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
        );

        thread::sleep(Duration::from_millis(200));

        // Register, then drop the connection.
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write_packet(
            &mut stream,
            PacketType::Name,
            &identity_payload("Ada", "STU-1"),
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
        );

        thread::sleep(Duration::from_millis(200));

        // First connection registers and drops.
        let mut first = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write_packet(&mut first, PacketType::Name, &identity_payload("Ada", "STU-9")).unwrap();
        thread::sleep(Duration::from_millis(150));
        drop(first);
        thread::sleep(Duration::from_millis(300));
        assert_eq!(runtime.snapshot().students.len(), 1);

        // Same student reconnects — should collapse to one live tile.
        let mut second = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write_packet(&mut second, PacketType::Name, &identity_payload("Ada", "STU-9")).unwrap();
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
        );

        thread::sleep(Duration::from_millis(200));

        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write_packet(
            &mut stream,
            PacketType::Name,
            &identity_payload("Probe Student", "PROBE-1"),
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
}
