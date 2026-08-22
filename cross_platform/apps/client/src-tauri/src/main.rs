// Hide the Windows console window in release builds. Without this attribute
// the packaged .exe is built for the console subsystem and pops a terminal.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use exam_monitor_core::{port_for_code, ClientRuntime, ClientSnapshot};
use std::net::IpAddr;
use std::sync::Mutex;
use tauri::State;

#[derive(Default)]
struct ClientState {
    runtime: Mutex<Option<ClientRuntime>>,
}

#[tauri::command(rename_all = "camelCase")]
fn start_client(
    state: State<'_, ClientState>,
    student_name: String,
    student_id: String,
    server_ip: Option<String>,
    join_code: String,
) -> Result<(), String> {
    let server_ip = parse_server_ip(server_ip.as_deref())?;
    let join_code = join_code.trim().to_uppercase();
    if join_code.is_empty() {
        return Err(String::from(
            "enter the class code shown on the teacher's screen",
        ));
    }
    // The code alone identifies the room — the port is derived from it.
    let port = port_for_code(&join_code);
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| String::from("client state is unavailable"))?;

    if let Some(mut existing) = runtime.take() {
        existing.stop();
    }

    *runtime = Some(ClientRuntime::start(
        student_name,
        student_id,
        port,
        server_ip,
        join_code,
    ));
    Ok(())
}

#[tauri::command]
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn parse_server_ip(value: Option<&str>) -> Result<Option<IpAddr>, String> {
    match value.map(str::trim) {
        None | Some("") => Ok(None),
        Some(text) => text
            .parse::<IpAddr>()
            .map(Some)
            .map_err(|_| String::from("server IP must be a valid IP address, e.g. 192.168.0.10")),
    }
}

#[tauri::command]
fn stop_client(state: State<'_, ClientState>) -> Result<(), String> {
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| String::from("client state is unavailable"))?;

    if let Some(mut existing) = runtime.take() {
        existing.stop();
    }

    Ok(())
}

#[tauri::command]
fn client_status(state: State<'_, ClientState>) -> Result<ClientSnapshot, String> {
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| String::from("client state is unavailable"))?;

    Ok(runtime
        .as_ref()
        .map(ClientRuntime::snapshot)
        .unwrap_or(ClientSnapshot {
            is_running: false,
            is_connected: false,
            status: String::from("Not connected"),
        }))
}

fn main() {
    tauri::Builder::default()
        .manage(ClientState::default())
        .invoke_handler(tauri::generate_handler![
            start_client,
            stop_client,
            client_status,
            app_version
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Exam Guard Client");
}
