use crate::discovery::{discovery_targets, DISCOVER_MESSAGE, SERVER_MESSAGE};
use crate::protocol::{parse_identity, read_packet, PacketType};
use base64::{engine::general_purpose, Engine};
use serde::Serialize;
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize)]
pub struct StudentSnapshot {
    pub id: u64,
    pub name: String,
    pub student_id: String,
    pub image_data_url: Option<String>,
    pub last_update_ms: u128,
}

#[derive(Clone, Debug, Serialize)]
pub struct ServerSnapshot {
    pub is_running: bool,
    pub exam_name: String,
    pub course_name: String,
    pub room_number: String,
    pub students: Vec<StudentSnapshot>,
}

pub struct ServerRuntime {
    running: Arc<AtomicBool>,
    students: Arc<Mutex<Vec<StudentSnapshot>>>,
    exam_name: String,
    course_name: String,
    room_number: String,
    workers: Vec<JoinHandle<()>>,
}

impl ServerRuntime {
    pub fn start(exam_name: String, course_name: String, room_number: String, port: u16) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let students = Arc::new(Mutex::new(Vec::new()));

        let accept_worker = {
            let running = Arc::clone(&running);
            let students = Arc::clone(&students);

            thread::spawn(move || run_tcp_listener(port, running, students))
        };

        let broadcast_worker = {
            let running = Arc::clone(&running);
            thread::spawn(move || run_udp_discovery(port, running))
        };

        Self {
            running,
            students,
            exam_name,
            course_name,
            room_number,
            workers: vec![accept_worker, broadcast_worker],
        }
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);

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
                let id = next_id;
                next_id += 1;
                eprintln!("SERVER: accepted TCP connection {id} from {peer_addr}");

                if let Err(error) = stream.set_nonblocking(false) {
                    eprintln!("SERVER: failed to set connection {id} blocking mode: {error}");
                    continue;
                }

                let running = Arc::clone(&running);
                let students = Arc::clone(&students);
                thread::spawn(move || handle_student(id, stream, running, students));
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
) {
    let mut is_registered = false;
    let mut received_frames = 0_u64;

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
                upsert_student(&students, id, |student| {
                    student.name = name;
                    student.student_id = student_id;
                    student.last_update_ms = now_ms();
                });
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

    if let Ok(mut students) = students.lock() {
        students.retain(|student| student.id != id);
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
            students.push(StudentSnapshot {
                id,
                name: String::from("Unknown"),
                student_id: String::new(),
                image_data_url: None,
                last_update_ms: now_ms(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{identity_payload, write_packet};
    use std::net::TcpStream;

    #[test]
    fn server_answers_udp_discovery_probe() {
        let port = 42_351;
        let mut runtime = ServerRuntime::start(
            String::from("Exam"),
            String::from("Course"),
            port.to_string(),
            port,
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
