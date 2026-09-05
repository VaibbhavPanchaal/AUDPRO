// src-tauri/src/sidecar.rs

use std::sync::Mutex;

use serde::Serialize;
use tauri::{
    async_runtime,
    AppHandle,
    Emitter,
    Manager,
    RunEvent,
    State,
};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

use crate::sessions::{valid_session_id, SessionStore};

/// Progress events emitted to the frontend over the `processing-progress`
/// channel. Mirrors the sidecar's NDJSON protocol one-to-one.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProcessingEvent {
    /// Pipeline stage marker: probe | transcribe | align | diarize | summarize
    Stage {
        stage: String,
        message: String,
    },
    Progress {
        stage: String,
        progress: f64,
        message: String,
    },
    Done {
        session_id: String,
        duration_seconds: f64,
        speaker_count: u32,
        language: Option<String>,
    },
    Error {
        stage: Option<String>,
        message: String,
    },
    /// Fallback for any line that did not parse as one of the above.
    Raw {
        message: String,
    },
}

#[derive(Default)]
pub struct SidecarState {
    child: Mutex<Option<CommandChild>>,
}

impl SidecarState {
    fn terminate(&self) {
        let child = match self.child.lock() {
            Ok(mut guard) => guard.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };

        if let Some(child) = child {
            if let Err(error) = child.kill() {
                eprintln!("Unable to terminate audio sidecar: {error}");
            }
        }
    }

    fn is_running(&self) -> bool {
        match self.child.lock() {
            Ok(guard) => guard.is_some(),
            Err(poisoned) => poisoned.into_inner().is_some(),
        }
    }

    fn set_child(&self, child: CommandChild) -> Result<(), String> {
        let mut guard = self
            .child
            .lock()
            .map_err(|_| "Sidecar state lock poisoned".to_string())?;

        if guard.is_some() {
            return Err("Audio processing is already running".into());
        }

        *guard = Some(child);
        Ok(())
    }

    fn clear_child(&self) {
        if let Ok(mut guard) = self.child.lock() {
            *guard = None;
        }
    }

    /// Take ownership of the current child (if any) so the caller can kill it.
    fn take_child_for_kill(&self) -> Option<CommandChild> {
        match self.child.lock() {
            Ok(mut guard) => guard.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        }
    }

    /// Dispatch one raw JSON command line to the sidecar's stdin. The
    /// child is taken out of the slot only for the duration of the write.
    /// (Requires `CommandChild: Write`, which tauri-plugin-shell provides.)
    pub fn send_sidecar_command(&self, command: &str) -> Result<(), String> {
        let mut line = command.trim().to_string();
        line.push('\n');

        let mut guard = self
            .child
            .lock()
            .map_err(|_| "Sidecar state lock poisoned".to_string())?;

        // Temporarily swap the child out so we can write to its stdin
        // through a mutable borrow without holding closure state.
        let mut child = guard
            .take()
            .ok_or_else(|| "No active sidecar to send commands to".to_string())?;

        let result = child
            .write(line.as_bytes())
            .map(|_| ())
            .map_err(|e| e.to_string());

        *guard = Some(child);
        result
    }
}

#[derive(serde::Deserialize)]
struct SidecarLine {
    #[serde(default)]
    event: Option<String>,
    #[serde(default)]
    payload: Option<serde_json::Value>,
}

fn parse_line(line: &str, session_id: &str) -> ProcessingEvent {
    let parsed: Option<SidecarLine> = serde_json::from_str(line).ok();

    let Some(event) = parsed else {
        return ProcessingEvent::Raw {
            message: line.to_string(),
        };
    };

    let payload = event.payload.unwrap_or(serde_json::Value::Null);
    let stage = payload
        .get("stage")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let message = payload
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    match event.event.as_deref() {
        Some("STAGE") => ProcessingEvent::Stage { stage, message },
        Some("PROGRESS") => ProcessingEvent::Progress {
            progress: payload
                .get("progress")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
                .clamp(0.0, 100.0),
            stage,
            message,
        },
        Some("COMPLETE") => {
            let status = payload
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("completed");

            if status == "completed" {
                ProcessingEvent::Done {
                    session_id: payload
                        .get("sessionId")
                        .and_then(|v| v.as_str())
                        .unwrap_or(session_id)
                        .to_string(),
                    duration_seconds: payload
                        .get("duration")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0),
                    speaker_count: payload
                        .get("speakers")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32,
                    language: payload
                        .get("language")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                }
            } else {
                ProcessingEvent::Error {
                    stage: Some("complete".into()),
                    message: format!("Pipeline reported status: {status}"),
                }
            }
        }
        Some("ERROR") => ProcessingEvent::Error {
            stage: payload.get("stage").and_then(|v| v.as_str()).map(String::from),
            message: if message.is_empty() {
                payload
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown sidecar error")
                    .to_string()
            } else {
                message
            },
        },
        // READY | PONG | BYE and anything else: informational.
        _ => ProcessingEvent::Raw {
            message: line.to_string(),
        },
    }
}

/// Spawn the `audio-processor` sidecar for a session and stream its NDJSON
/// protocol lines to the frontend as typed `processing-progress` events.
#[tauri::command]
pub async fn process_audio(
    app: AppHandle,
    state: State<'_, SidecarState>,
    session_id: String,
    models_dir: Option<String>,
    language: Option<String>,
    device: Option<String>,
) -> Result<(), String> {
    if !valid_session_id(&session_id) {
        return Err("Invalid session ID".into());
    }

    if state.is_running() {
        return Err("Audio processing is already running".into());
    }

    let audio_path = {
        let store = app.state::<SessionStore>();
        let dir = store.session_dir(&session_id);
        let audio = dir.join(crate::sessions::AUDIO_FILE);

        if !audio.is_file() {
            return Err(format!(
                "No audio.wav found for session '{session_id}'. Record audio first."
            ));
        }

        audio
    };

    // Resolve the frozen sidecar bundled via `bundle.resources`:
    //   <resource_dir>/sidecar/<platform binary name>
    // (onedir layout stays intact — PyInstaller's `_internal/` ships
    // alongside the launcher inside the resource folder.)
    let sidecar_name = if cfg!(target_os = "windows") {
        "audio-processor-x86_64-pc-windows-msvc.exe"
    } else {
        "audio-processor-x86_64-apple-darwin"
    };

    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| format!("Unable to resolve resource directory: {e}"))?;

    let sidecar_path = resource_dir.join("sidecar").join(sidecar_name);

    if !sidecar_path.is_file() {
        return Err(format!(
            "Frozen sidecar not found at {}. Build it first: \
             cd python-sidecar && bash build-mac-intel.sh (macOS) or \
             powershell -File build-windows.ps1 (Windows).",
            sidecar_path.display()
        ));
    }

    let command = app.shell().command(sidecar_path);

    let (mut events, child) = command
        .spawn()
        .map_err(|error| format!("Unable to start audio sidecar: {error}"))?;

    if let Err(e) = state.set_child(child) {
        let _ = state.take_child_for_kill();
        return Err(e);
    }

    let dispatch = serde_json::json!({
        "command": "PROCESS",
        "audioPath": audio_path.to_string_lossy(),
        "outputDir": audio_path.parent().map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        "sessionId": session_id,
        "modelsDir": models_dir,
        "language": language,
        "device": device,
    });

    if let Err(e) = state.send_sidecar_command(&dispatch.to_string()) {
        return Err(format!("Unable to dispatch PROCESS command: {e}"));
    }

    let app_handle = app.clone();
    let session = session_id.clone();

    async_runtime::spawn(async move {
        let mut stdout_buffer = String::new();

        while let Some(event) = events.recv().await {
            match event {
                CommandEvent::Stdout(bytes) => {
                    stdout_buffer.push_str(&String::from_utf8_lossy(&bytes));

                    while let Some(newline_index) = stdout_buffer.find('\n') {
                        let line = stdout_buffer[..newline_index]
                            .trim_end_matches('\r')
                            .to_owned();
                        stdout_buffer.drain(..=newline_index);

                        if line.is_empty() {
                            continue;
                        }

                        let payload = parse_line(&line, &session);
                        let _ = app_handle.emit("processing-progress", payload);
                    }
                }

                CommandEvent::Stderr(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    // Diagnostics only; forward to the app log, not the UI.
                    eprint!("{text}");
                }

                CommandEvent::Terminated(payload) => {
                    // Flush any trailing line without a newline.
                    let remainder = stdout_buffer.trim();
                    if !remainder.is_empty() {
                        let payload = parse_line(remainder, &session);
                        let _ = app_handle.emit("processing-progress", payload);
                    }

                    if let Some(code) = payload.code {
                        if code != 0 {
                            let _ = app_handle.emit(
                                "processing-progress",
                                ProcessingEvent::Error {
                                    stage: None,
                                    message: format!("Sidecar exited with code {code}"),
                                },
                            );
                        }
                    }

                    break;
                }

                CommandEvent::Error(error) => {
                    let _ = app_handle.emit(
                        "processing-progress",
                        ProcessingEvent::Error {
                            stage: None,
                            message: format!("Sidecar transport error: {error}"),
                        },
                    );
                }

                _ => {}
            }
        }

        if let Some(state) = app_handle.try_state::<SidecarState>() {
            state.clear_child();
        }
    });

    Ok(())
}

/// Cancel a running sidecar, if any.
#[tauri::command]
pub fn cancel_processing(state: State<'_, SidecarState>) -> Result<(), String> {
    if !state.is_running() {
        return Err("No processing run to cancel".into());
    }

    state.terminate();
    Ok(())
}

pub fn terminate_on_exit(app_handle: &tauri::AppHandle) {
    if let Some(state) = app_handle.try_state::<SidecarState>() {
        state.terminate();
    }
}

pub fn run_event_handler(app_handle: &tauri::AppHandle, event: RunEvent) {
    if matches!(event, RunEvent::ExitRequested { .. } | RunEvent::Exit) {
        terminate_on_exit(app_handle);
    }
}