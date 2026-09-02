// src-tauri/src/lib.rs

mod audio;
mod commands;
mod sessions;
mod sidecar;
mod state;

use tauri::Manager;

use commands::audio::{
    get_session,
    is_recording,
    list_input_devices,
    list_sessions,
    open_session_folder,
    start_recording,
    stop_recording,
};
use sessions::SessionStore;
use sidecar::{cancel_processing, process_audio, run_event_handler, SidecarState};
use state::app_state::RecorderState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let sessions_root = app
                .path()
                .app_data_dir()
                .expect("Unable to resolve app data directory")
                .join("sessions");

            std::fs::create_dir_all(&sessions_root)
                .expect("Unable to create sessions directory");

            app.manage(SessionStore::new(sessions_root));
            Ok(())
        })
        .manage(RecorderState::default())
        .manage(SidecarState::default())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            is_recording,
            list_input_devices,
            list_sessions,
            get_session,
            open_session_folder,
            process_audio,
            cancel_processing,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Tauri application")
        .run(run_event_handler);
}