// Hide the Windows console window in release builds. Without this attribute
// the packaged .exe is built for the console subsystem and pops a terminal.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use exam_monitor_core::{generate_join_code, port_for_code, ServerRuntime, ServerSnapshot};
use std::net::TcpListener;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, RunEvent, State};

#[derive(Default)]
struct ServerState {
    runtime: Mutex<Option<ServerRuntime>>,
}

#[tauri::command(rename_all = "camelCase")]
fn start_server(
    app: AppHandle,
    state: State<'_, ServerState>,
    exam_name: String,
    course_name: String,
) -> Result<(), String> {
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| String::from("server state is unavailable"))?;

    if let Some(mut existing) = runtime.take() {
        existing.stop();
    }

    // Evidence log goes under the app's per-user log dir; if it can't be
    // resolved we run without logging rather than failing to start.
    let log_base_dir = app.path().app_log_dir().ok();

    // The class code is the room's only identifier: students type it, and the
    // port is derived from it. Re-roll if that port is already in use so a
    // clash heals itself instead of surfacing an error to the teacher.
    let (join_code, port) = pick_available_code()?;

    *runtime = Some(ServerRuntime::start(
        exam_name,
        course_name,
        join_code.clone(),
        port,
        log_base_dir,
        join_code,
    ));
    Ok(())
}

/// Generate a class code whose derived port we can actually bind.
fn pick_available_code() -> Result<(String, u16), String> {
    for _ in 0..10 {
        let code = generate_join_code();
        let port = port_for_code(&code);
        // Probe-and-release: the real listener binds moments later. The race
        // window is negligible for one teacher starting one room.
        if TcpListener::bind(("0.0.0.0", port)).is_ok() {
            return Ok((code, port));
        }
    }
    Err(String::from(
        "could not find a free class code, please try again",
    ))
}

#[tauri::command(rename_all = "camelCase")]
fn set_clear_evidence(state: State<'_, ServerState>, enabled: bool) -> Result<(), String> {
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| String::from("server state is unavailable"))?;

    if let Some(runtime) = runtime.as_ref() {
        runtime.set_clear_evidence(enabled);
    }

    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
fn dismiss_student(state: State<'_, ServerState>, student_id: u64) -> Result<(), String> {
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| String::from("server state is unavailable"))?;

    if let Some(runtime) = runtime.as_ref() {
        runtime.dismiss_student(student_id);
    }

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
            log_dir: None,
            join_code: String::new(),
            port: 0,
            wrong_code_attempts: 0,
            unreachable_devices: 0,
        }))
}

#[tauri::command]
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn main() {
    tauri::Builder::default()
        .manage(ServerState::default())
        .invoke_handler(tauri::generate_handler![
            start_server,
            stop_server,
            server_status,
            dismiss_student,
            set_clear_evidence,
            app_version
        ])
        .build(tauri::generate_context!())
        .expect("failed to run Exam Guard Server")
        .run(|app, event| {
            // On quit, honour the clear-evidence opt-in. Tauri often hard-exits
            // without running Drop, so we clear here before the process ends.
            if let RunEvent::ExitRequested { .. } = event {
                if let Some(state) = app.try_state::<ServerState>() {
                    if let Ok(runtime) = state.runtime.lock() {
                        if let Some(runtime) = runtime.as_ref() {
                            runtime.clear_evidence();
                        }
                    }
                }
            }
        });
}
