// src-tauri/src/sessions.rs

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Files required inside every session directory, per the data schema:
/// audio.wav, transcript.json, metadata.json, summary.md
pub const AUDIO_FILE: &str = "audio.wav";
pub const TRANSCRIPT_FILE: &str = "transcript.json";
pub const METADATA_FILE: &str = "metadata.json";
pub const SUMMARY_FILE: &str = "summary.md";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub uuid: String,
    #[serde(rename = "timestamp")]
    pub created_at: String,
    pub duration_seconds: f64,
    pub speaker_count: u32,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionSummary {
    pub id: String,
    pub created_at: String,
    pub duration_seconds: f64,
    pub speaker_count: u32,
    pub status: String,
    pub language: Option<String>,
    pub has_summary: bool,
    pub has_transcript: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionDetail {
    pub id: String,
    pub metadata: SessionMetadata,
    pub audio_path: String,
    pub summary: Option<String>,
    pub transcript: Option<serde_json::Value>,
}

/// Root directory that holds all `<uuid>/` session folders.
pub struct SessionStore {
    root: PathBuf,
}

impl SessionStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// `<root>/<session_id>`
    pub fn session_dir(&self, session_id: &str) -> PathBuf {
        self.root.join(session_id)
    }

    /// Create (and return) the directory for a new session.
    pub fn create_session_dir(&self, session_id: &str) -> Result<PathBuf, String> {
        if !valid_session_id(session_id) {
            return Err("Invalid session ID".into());
        }

        let dir = self.session_dir(session_id);
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Unable to create session directory: {e}"))?;

        Ok(dir)
    }

    pub fn list_sessions(&self) -> Vec<SessionSummary> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(_) => return Vec::new(),
        };

        let mut sessions: Vec<SessionSummary> = entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| self.read_session_summary(&entry.path()))
            .collect();

        // Newest first.
        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        sessions
    }

    pub fn read_session_detail(&self, session_id: &str) -> Result<SessionDetail, String> {
        if !valid_session_id(session_id) {
            return Err("Invalid session ID".into());
        }

        let dir = self.session_dir(session_id);

        if !dir.is_dir() {
            return Err(format!("Session '{session_id}' not found"));
        }

        let metadata = self
            .read_metadata(&dir)
            .ok_or_else(|| format!("Session '{session_id}' has no metadata.json"))?;

        let audio_path = dir.join(AUDIO_FILE);
        let summary = fs::read_to_string(dir.join(SUMMARY_FILE)).ok();
        let transcript = fs::read_to_string(dir.join(TRANSCRIPT_FILE))
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok());

        Ok(SessionDetail {
            id: session_id.to_string(),
            metadata,
            audio_path: audio_path.to_string_lossy().into_owned(),
            summary,
            transcript,
        })
    }

    /// Write/refresh metadata.json for a session (used at record start).
    pub fn write_initial_metadata(&self, session_id: &str) -> Result<(), String> {
        let metadata = SessionMetadata {
            uuid: session_id.to_string(),
            created_at: iso8601_now(),
            duration_seconds: 0.0,
            speaker_count: 0,
            status: "recording".to_string(),
            language: None,
        };

        self.write_metadata(session_id, &metadata)
    }

    pub fn write_metadata(
        &self,
        session_id: &str,
        metadata: &SessionMetadata,
    ) -> Result<(), String> {
        let dir = self.session_dir(session_id);
        let path = dir.join(METADATA_FILE);
        let json = serde_json::to_string_pretty(metadata)
            .map_err(|e| format!("Unable to serialize metadata: {e}"))?;

        fs::write(path, json).map_err(|e| format!("Unable to write metadata.json: {e}"))
    }

    fn read_session_summary(&self, dir: &Path) -> Option<SessionSummary> {
        let metadata = self.read_metadata(dir)?;
        let id = dir.file_name()?.to_string_lossy().into_owned();

        Some(SessionSummary {
            has_summary: dir.join(SUMMARY_FILE).is_file(),
            has_transcript: dir.join(TRANSCRIPT_FILE).is_file(),
            id,
            created_at: metadata.created_at,
            duration_seconds: metadata.duration_seconds,
            speaker_count: metadata.speaker_count,
            status: metadata.status,
            language: metadata.language,
        })
    }

    fn read_metadata(&self, dir: &Path) -> Option<SessionMetadata> {
        let raw = fs::read_to_string(dir.join(METADATA_FILE)).ok()?;
        serde_json::from_str(&raw).ok()
    }
}

/// Session IDs are used as directory names; lock them down to a safe set.
pub fn valid_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id.len() <= 128
        && session_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

pub fn iso8601_now() -> String {
    // RFC 3339 UTC timestamp without external crates: derive from the
    // Unix epoch using civil-date arithmetic (Howard Hinnant's algorithm).
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();

    let secs = now.as_secs() as i64;
    let millis = now.subsec_millis();

    let days = secs.div_euclid(86_400);
    let secs_of_day = secs.rem_euclid(86_400);

    let (year, month, day) = civil_from_days(days);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);

    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;

    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_validation() {
        assert!(valid_session_id("session-1730000000000"));
        assert!(valid_session_id("abc_DEF-123"));
        assert!(!valid_session_id(""));
        assert!(!valid_session_id("../escape"));
        assert!(!valid_session_id("with space"));
        assert!(!valid_session_id(&"x".repeat(129)));
    }

    #[test]
    fn iso8601_format() {
        let stamp = iso8601_now();
        assert_eq!(stamp.len(), 24);
        assert!(stamp.ends_with('Z'));
        assert_eq!(&stamp[4..5], "-");
        assert_eq!(&stamp[10..11], "T");
    }

    #[test]
    fn list_and_read_sessions_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("tauri-sessions-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);

        let store = SessionStore::new(tmp.clone());
        let dir = store.create_session_dir("sess-1").unwrap();
        assert!(dir.join("audio.wav").is_file() == false);

        store
            .write_metadata(
                "sess-1",
                &SessionMetadata {
                    uuid: "sess-1".into(),
                    created_at: "2026-01-01T00:00:00.000Z".into(),
                    duration_seconds: 12.5,
                    speaker_count: 2,
                    status: "completed".into(),
                    language: Some("EN".into()),
                },
            )
            .unwrap();

        fs::write(dir.join(SUMMARY_FILE), "# Summary").unwrap();

        let list = store.list_sessions();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "sess-1");
        assert!(list[0].has_summary);

        let detail = store.read_session_detail("sess-1").unwrap();
        assert_eq!(detail.metadata.speaker_count, 2);
        assert_eq!(detail.summary.as_deref(), Some("# Summary"));

        assert!(store.read_session_detail("../escape").is_err());
        assert!(store.read_session_detail("missing").is_err());

        let _ = fs::remove_dir_all(&tmp);
    }
}