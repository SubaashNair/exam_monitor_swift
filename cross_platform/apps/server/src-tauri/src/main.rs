// Hide the Windows console window in release builds. Without this attribute
// the packaged .exe is built for the console subsystem and pops a terminal.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use exam_monitor_core::{ServerRuntime, ServerSnapshot};
use std::sync::Mutex;
use tauri::State;

#[derive(Default)]
struct ServerState {
    runtime: Mutex<Option<ServerRuntime>>,
}

#[tauri::command(rename_all = "camelCase")]
fn start_server(
    state: State<'_, ServerState>,
    exam_name: String,
    course_name: String,
    room_number: String,
) -> Result<(), String> {
    let port = parse_room_number(&room_number)?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| String::from("server state is unavailable"))?;

    if let Some(mut existing) = runtime.take() {
        existing.stop();
    }

    *runtime = Some(ServerRuntime::start(
        exam_name,
        course_name,
        room_number,
        port,
    ));
    Ok(())
}

#[tauri::command]
fn stop_server(state: State<'_, ServerState>) -> Result<(), String> {
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| String::from("server state is unavailable"))?;

    if let Some(mut existing) = runtime.take() {
        existing.stop();
    }

    Ok(())
}

#[tauri::command]
fn server_status(state: State<'_, ServerState>) -> Result<ServerSnapshot, String> {
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| String::from("server state is unavailable"))?;

    Ok(runtime
        .as_ref()
        .map(ServerRuntime::snapshot)
        .unwrap_or(ServerSnapshot {
            is_running: false,
            exam_name: String::new(),
            course_name: String::new(),
            room_number: String::new(),
            students: Vec::new(),
        }))
}

fn parse_room_number(value: &str) -> Result<u16, String> {
    let port: u16 = value
        .parse()
        .map_err(|_| String::from("class number must be a valid port"))?;

    if port == 0 {
        return Err(String::from("class number must be greater than zero"));
    }

    Ok(port)
}

fn main() {
    tauri::Builder::default()
        .manage(ServerState::default())
        .invoke_handler(tauri::generate_handler![
            start_server,
            stop_server,
            server_status
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Exam Guard Server");
}
