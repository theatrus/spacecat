//! Persistent state for the bot's live-status messages (Phase 2).
//!
//! For each telescope that has a status message pinned in its channel we
//! remember the `(channel_id, message_id)` pair so subsequent poll cycles
//! can edit the same message in place rather than spamming the channel
//! with fresh posts. Stored at `chat.discord_bot.state_file` (default
//! `./spacecat-state.json`).
//!
//! Atomic writes: serialize to a tempfile alongside the target, then
//! rename in place. A crash during write leaves the previous valid file
//! intact.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StatusMessage {
    pub channel_id: u64,
    pub message_id: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatusState {
    /// telescope name -> live status message reference
    #[serde(default)]
    pub status_messages: HashMap<String, StatusMessage>,
}

impl StatusState {
    /// Load state from `path`. Returns `Default` when the file is missing
    /// (first run). Other I/O or parse errors propagate.
    pub fn load(path: &Path) -> io::Result<Self> {
        match fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).map_err(io::Error::other),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// Atomic save: write to `<path>.tmp` then rename to `path`. The
    /// rename is atomic on POSIX and on Windows for files in the same
    /// directory.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let mut temp: PathBuf = path.to_path_buf();
        let fname = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "state".to_string());
        temp.set_file_name(format!(".{fname}.tmp"));
        fs::write(&temp, json)?;
        fs::rename(&temp, path)?;
        Ok(())
    }

    pub fn get(&self, telescope: &str) -> Option<StatusMessage> {
        self.status_messages.get(telescope).copied()
    }

    pub fn set(&mut self, telescope: &str, message: StatusMessage) {
        self.status_messages.insert(telescope.to_string(), message);
    }

    pub fn remove(&mut self, telescope: &str) {
        self.status_messages.remove(telescope);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn tmp_path(suffix: &str) -> PathBuf {
        let mut p = env::temp_dir();
        p.push(format!(
            "spacecat-status-{}-{}.json",
            std::process::id(),
            suffix
        ));
        p
    }

    #[test]
    fn test_load_missing_returns_default() {
        let p = tmp_path("missing");
        let _ = fs::remove_file(&p);
        let state = StatusState::load(&p).unwrap();
        assert!(state.status_messages.is_empty());
    }

    #[test]
    fn test_save_and_roundtrip() {
        let p = tmp_path("roundtrip");
        let mut state = StatusState::default();
        state.set(
            "c925",
            StatusMessage {
                channel_id: 111,
                message_id: 999,
            },
        );
        state.save(&p).unwrap();

        let loaded = StatusState::load(&p).unwrap();
        let m = loaded.get("c925").unwrap();
        assert_eq!(m.channel_id, 111);
        assert_eq!(m.message_id, 999);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn test_remove() {
        let mut state = StatusState::default();
        state.set(
            "a",
            StatusMessage {
                channel_id: 1,
                message_id: 2,
            },
        );
        assert!(state.get("a").is_some());
        state.remove("a");
        assert!(state.get("a").is_none());
    }
}
