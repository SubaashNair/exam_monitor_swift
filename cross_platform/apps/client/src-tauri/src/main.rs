use exam_monitor_core::{ClientRuntime, ClientSnapshot};
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
    room_number: String,
) -> Result<(), String> {
    let port = parse_room_number(&room_number)?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| String::from("client state is unavailable"))?;

    if let Some(mut existing) = runtime.take() {
        existing.stop();
    }

    *runtime = Some(ClientRuntime::start(student_name, student_id, port));
    Ok(())
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
        .manage(ClientState::default())
        .invoke_handler(tauri::generate_handler![
            start_client,
            stop_client,
            client_status
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Exam Guard Client");
}
