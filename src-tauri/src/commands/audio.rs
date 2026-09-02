// src-tauri/src/commands/audio.rs

use tauri::command;
use tauri::AppHandle;
use tauri::Manager;
use tauri::State;

use crate::audio::recorder::Recorder;
use crate::state::app_state::RecorderState;
use crate::sessions::{valid_session_id, SessionStore};

/// Begin recording a new session into `<app_data>/sessions/<session_id>/audio.wav`.
#[command]
pub fn start_recording(
    app: AppHandle,
    state: State<'_, RecorderState>,
    session_id: String,
    device: Option<String>,
) -> Result<(), String> {
    if !valid_session_id(&session_id) {
        return Err("Invalid session ID".into());
    }

    let mut guard = state
        .active
        .lock()
        .map_err(|_| "Recorder state lock poisoned".to_string())?;

    if guard.is_some() {
        return Err("A recording is already active".into());
    }

    let store_root = app
        .state::<SessionStore>()
        .session_dir(&session_id);

    // Stamped metadata so the session shows up in history immediately.
    app.state::<SessionStore>()
        .write_initial_metadata(&session_id)?;

    let recorder = Recorder::start(&store_root, device.as_deref())
        .map_err(|e| {
            // Roll back the placeholder metadata if capture could not start.
            let _ = std::fs::remove_dir_all(&store_root);
            e
        })?;

    *guard = Some(recorder);
    Ok(())
}

/// Stop the active recording, finalize audio.wav, and update metadata with
/// the captured duration. Returns the session id on success.
#[command]
pub fn stop_recording(
    app: AppHandle,
    state: State<'_, RecorderState>,
) -> Result<String, String> {
    let recorder = state
        .active
        .lock()
        .map_err(|_| "Recorder state lock poisoned".to_string())?
        .take()
        .ok_or_else(|| "No active recording".to_string())?;

    let elapsed = recorder.elapsed().as_secs_f64();
    let output_path = recorder.stop()?;
    let session_id = output_path
        .rsplit(std::path::MAIN_SEPARATOR)
        .nth(1)
        .unwrap_or_default()
        .to_string();

    // Refresh metadata with the real duration and recording-complete status.
    let store = app.state::<SessionStore>();

    if let Ok(detail) = store.read_session_detail(&session_id) {
        let mut metadata = detail.metadata;
        metadata.duration_seconds = elapsed;
        metadata.status = "recorded".to_string();
        let _ = store.write_metadata(&session_id, &metadata);
    }

    Ok(session_id)
}

/// True while a capture is in flight; the UI uses this to disable controls.
#[command]
pub fn is_recording(state: State<'_, RecorderState>) -> bool {
    state
        .active
        .lock()
        .map(|guard| guard.is_some())
        .unwrap_or(false)
}

/// List input device names.
#[command]
pub fn list_input_devices() -> Vec<String> {
    crate::audio::recorder::list_input_devices()
}

#[command]
pub fn list_sessions(store: State<'_, SessionStore>) -> Vec<crate::sessions::SessionSummary> {
    store.list_sessions()
}

#[command]
pub fn get_session(
    store: State<'_, SessionStore>,
    session_id: String,
) -> Result<crate::sessions::SessionDetail, String> {
    store.read_session_detail(&session_id)
}

/// Open a session folder in the OS file explorer.
#[command]
pub fn open_session_folder(
    app: AppHandle,
    store: State<'_, SessionStore>,
    session_id: String,
) -> Result<(), String> {
    let dir = store.session_dir(&session_id);

    if !dir.is_dir() {
        return Err(format!("Session '{session_id}' not found"));
    }

    #[cfg(target_os = "windows")]
    let opened = std::process::Command::new("explorer")
        .arg(&dir)
        .spawn();

    #[cfg(target_os = "macos")]
    let opened = std::process::Command::new("open")
        .arg(&dir)
        .spawn();

    #[cfg(all(unix, not(target_os = "macos")))]
    let opened = std::process::Command::new("xdg-open")
        .arg(&dir)
        .spawn();

    match opened {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Unable to open session folder: {e}")),
    }
    .map(|_| {
        let _ = &app; // keeps the signature uniform across platforms
    })
}