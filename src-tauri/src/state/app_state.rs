// src-tauri/src/state/app_state.rs

use std::sync::Mutex;

use crate::audio::recorder::Recorder;

/// Holds the single active recorder, if a capture is running.
#[derive(Default)]
pub struct RecorderState {
    pub active: Mutex<Option<Recorder>>,
}