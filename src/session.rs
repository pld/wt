use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::config::{xdg_state_home, Config};
use crate::tmux_manager::TmuxManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub session_name: String,
    pub worktrees: HashMap<String, WindowInfo>,
    /// Windows-mode sessions keyed by worktree name. Empty for panes-only
    /// users, and absent from pre-windows-mode state files.
    #[serde(default)]
    pub windows_sessions: HashMap<String, WindowsSessionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub window_index: u32,
    pub pane_count: u8,
    pub worktree_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsSessionInfo {
    pub session_name: String,
    pub worktree_path: PathBuf,
    pub windows: Vec<String>,
}

impl SessionState {
    pub fn new(session_name: &str) -> Self {
        Self {
            session_name: session_name.to_string(),
            worktrees: HashMap::new(),
            windows_sessions: HashMap::new(),
        }
    }

    /// Path for reading state: new XDG location, falling back to legacy `~/.wt/sessions.json`
    /// with a one-time stderr notice if only the legacy path exists.
    fn state_read_path() -> Result<PathBuf> {
        let new_path = xdg_state_home()
            .ok_or_else(|| anyhow::anyhow!("Could not determine XDG_STATE_HOME"))?
            .join("wt/sessions.json");
        if !new_path.exists() {
            if let Some(legacy) = dirs::home_dir().map(|h| h.join(".wt/sessions.json")) {
                if legacy.exists() {
                    eprintln!(
                        "wt: notice: reading session state from legacy ~/.wt/sessions.json; rerun ./install.sh to migrate"
                    );
                    return Ok(legacy);
                }
            }
        }
        Ok(new_path)
    }

    /// Path for writing state: always the new XDG location (creates the directory).
    fn state_write_path() -> Result<PathBuf> {
        Ok(Config::ensure_state_dir()?.join("sessions.json"))
    }

    /// Load session state from the XDG state location (or legacy fallback).
    pub fn load() -> Result<Option<Self>> {
        let path = Self::state_read_path()?;
        if !path.exists() {
            return Ok(None);
        }

        let contents = std::fs::read_to_string(&path).context("Failed to read sessions.json")?;
        let state: SessionState =
            serde_json::from_str(&contents).context("Failed to parse sessions.json")?;

        Ok(Some(state))
    }

    /// Save session state to the XDG state location.
    pub fn save(&self) -> Result<()> {
        let path = Self::state_write_path()?;
        let contents =
            serde_json::to_string_pretty(self).context("Failed to serialize session state")?;

        std::fs::write(&path, contents).context("Failed to write sessions.json")?;
        Ok(())
    }

    /// Add a worktree window to the session
    pub fn add_worktree(&mut self, name: &str, window_index: u32, pane_count: u8, path: PathBuf) {
        self.worktrees.insert(
            name.to_string(),
            WindowInfo {
                window_index,
                pane_count,
                worktree_path: path,
            },
        );
    }

    /// Remove a worktree from the session
    pub fn remove_worktree(&mut self, name: &str) -> Option<WindowInfo> {
        self.worktrees.remove(name)
    }

    /// Get worktree info by name
    pub fn get_worktree(&self, name: &str) -> Option<&WindowInfo> {
        self.worktrees.get(name)
    }

    /// Check if a worktree is in the session
    pub fn has_worktree(&self, name: &str) -> bool {
        self.worktrees.contains_key(name)
    }

    /// Sync session state with actual tmux windows.
    pub fn sync_with_tmux(&mut self, tmux: &TmuxManager) -> Result<()> {
        let windows = tmux.list_windows()?;
        let window_names: HashSet<_> = windows.iter().map(|window| window.name.clone()).collect();

        self.worktrees.retain(|name, _| window_names.contains(name));

        for window in &windows {
            if let Some(info) = self.worktrees.get_mut(&window.name) {
                info.pane_count = window.pane_count as u8;
            }
        }

        Ok(())
    }

    /// Clear the session state file.
    pub fn clear() -> Result<()> {
        let path = Self::state_read_path()?;
        if path.exists() {
            std::fs::remove_file(&path).context("Failed to remove sessions.json")?;
        }
        Ok(())
    }

    /// Whether the state holds no panes-mode or windows-mode entries.
    pub fn is_empty(&self) -> bool {
        self.worktrees.is_empty() && self.windows_sessions.is_empty()
    }

    /// Drop all panes-mode entries while preserving windows-mode state.
    pub fn clear_panes_state(&mut self) {
        self.worktrees.clear();
    }

    /// Upsert a windows-mode session association.
    pub fn add_windows_session(&mut self, worktree: &str, info: WindowsSessionInfo) {
        self.windows_sessions.insert(worktree.to_string(), info);
    }

    /// Remove a windows-mode session association.
    pub fn remove_windows_session(&mut self, worktree: &str) -> Option<WindowsSessionInfo> {
        self.windows_sessions.remove(worktree)
    }
}

/// Drop windows-mode entries whose tmux session is no longer live.
pub fn retain_live_sessions(
    entries: &mut HashMap<String, WindowsSessionInfo>,
    live: &HashSet<String>,
) {
    entries.retain(|_, info| live.contains(&info.session_name));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_state_new() {
        let state = SessionState::new("wt");
        assert_eq!(state.session_name, "wt");
        assert!(state.worktrees.is_empty());
        assert!(state.windows_sessions.is_empty());
    }

    #[test]
    fn test_add_remove_worktree() {
        let mut state = SessionState::new("wt");
        state.add_worktree("feature-1", 1, 2, PathBuf::from("/path/to/feature-1"));

        assert!(state.has_worktree("feature-1"));
        assert!(!state.has_worktree("feature-2"));

        let info = state.get_worktree("feature-1").unwrap();
        assert_eq!(info.window_index, 1);
        assert_eq!(info.pane_count, 2);

        state.remove_worktree("feature-1");
        assert!(!state.has_worktree("feature-1"));
    }

    #[test]
    fn test_serialize_deserialize() {
        let mut state = SessionState::new("wt");
        state.add_worktree("feature-1", 1, 3, PathBuf::from("/path/to/feature-1"));

        let json = serde_json::to_string(&state).unwrap();
        let loaded: SessionState = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.session_name, "wt");
        assert!(loaded.has_worktree("feature-1"));
    }

    #[test]
    fn test_add_remove_windows_session() {
        let mut state = SessionState::new("wt");
        let info = WindowsSessionInfo {
            session_name: "wt-feature".to_string(),
            worktree_path: PathBuf::from("/path/to/feature"),
            windows: vec!["agent".into(), "shell".into()],
        };

        state.add_windows_session("feature", info.clone());

        assert_eq!(state.windows_sessions.get("feature"), Some(&info));
        assert_eq!(state.remove_windows_session("feature"), Some(info));
        assert!(state.windows_sessions.is_empty());
    }

    #[test]
    fn test_windows_session_serde_round_trip() {
        let mut state = SessionState::new("wt");
        state.add_windows_session(
            "feature",
            WindowsSessionInfo {
                session_name: "wt-feature".to_string(),
                worktree_path: PathBuf::from("/path/to/feature"),
                windows: vec!["agent".into(), "shell".into(), "edit".into()],
            },
        );

        let json = serde_json::to_string(&state).unwrap();
        let loaded: SessionState = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.windows_sessions, state.windows_sessions);
    }

    #[test]
    fn test_deserialize_legacy_state_without_windows_sessions() {
        let legacy = r#"{
            "session_name": "wt",
            "worktrees": {}
        }"#;

        let state: SessionState = serde_json::from_str(legacy).unwrap();
        assert_eq!(state.session_name, "wt");
        assert!(state.windows_sessions.is_empty());
    }

    #[test]
    fn test_retain_live_sessions_drops_stale_entries() {
        let mut entries = HashMap::new();
        entries.insert(
            "alive".to_string(),
            WindowsSessionInfo {
                session_name: "wt-alive".to_string(),
                worktree_path: PathBuf::from("/p/alive"),
                windows: vec!["agent".into(), "shell".into()],
            },
        );
        entries.insert(
            "stale".to_string(),
            WindowsSessionInfo {
                session_name: "wt-stale".to_string(),
                worktree_path: PathBuf::from("/p/stale"),
                windows: vec!["agent".into(), "shell".into()],
            },
        );

        let live: HashSet<String> = ["wt-alive".to_string()].into_iter().collect();
        retain_live_sessions(&mut entries, &live);

        assert_eq!(entries.len(), 1);
        assert!(entries.contains_key("alive"));
        assert!(!entries.contains_key("stale"));
    }

    #[test]
    fn test_clear_panes_state_preserves_windows_sessions() {
        let mut state = SessionState::new("wt");
        state.add_worktree("feature", 1, 2, PathBuf::from("/path/feature"));
        state.add_windows_session(
            "other",
            WindowsSessionInfo {
                session_name: "wt-other".to_string(),
                worktree_path: PathBuf::from("/path/other"),
                windows: vec!["agent".into(), "shell".into()],
            },
        );

        state.clear_panes_state();

        assert!(state.worktrees.is_empty());
        assert!(state.windows_sessions.contains_key("other"));
        assert!(!state.is_empty());
    }

    #[test]
    fn test_is_empty() {
        let mut state = SessionState::new("wt");
        assert!(state.is_empty());
        state.add_worktree("feature", 1, 2, PathBuf::from("/path/feature"));
        assert!(!state.is_empty());
        state.clear_panes_state();
        assert!(state.is_empty());
    }

    #[test]
    fn test_retain_live_sessions_empty_live_set_clears_all() {
        let mut entries = HashMap::new();
        entries.insert(
            "foo".to_string(),
            WindowsSessionInfo {
                session_name: "wt-foo".to_string(),
                worktree_path: PathBuf::from("/p/foo"),
                windows: vec![],
            },
        );

        retain_live_sessions(&mut entries, &HashSet::new());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_legacy_state_fallback_is_read_when_xdg_path_absent() {
        use std::io::Write;

        let _e = crate::ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let legacy_dir = dir.path().join(".wt");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        let legacy_path = legacy_dir.join("sessions.json");

        let state = SessionState::new("wt-legacy");
        let json = serde_json::to_string(&state).unwrap();
        std::fs::File::create(&legacy_path)
            .unwrap()
            .write_all(json.as_bytes())
            .unwrap();

        std::env::set_var("HOME", dir.path());
        std::env::remove_var("XDG_STATE_HOME");

        let loaded = SessionState::load().unwrap();
        std::env::remove_var("HOME");

        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().session_name, "wt-legacy");
    }
}
