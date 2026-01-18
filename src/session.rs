use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::Config;
use crate::tmux_manager::TmuxManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub session_name: String,
    pub worktrees: HashMap<String, WindowInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub window_index: u32,
    pub pane_count: u8,
    pub worktree_path: PathBuf,
}

impl SessionState {
    pub fn new(session_name: &str) -> Self {
        Self {
            session_name: session_name.to_string(),
            worktrees: HashMap::new(),
        }
    }

    fn state_file_path() -> Result<PathBuf> {
        let wt_dir = Config::ensure_wt_dir()?;
        Ok(wt_dir.join("sessions.json"))
    }

    /// Load session state from ~/.wt/sessions.json
    pub fn load() -> Result<Option<Self>> {
        let path = Self::state_file_path()?;
        if !path.exists() {
            return Ok(None);
        }

        let contents = std::fs::read_to_string(&path).context("Failed to read sessions.json")?;

        let state: SessionState =
            serde_json::from_str(&contents).context("Failed to parse sessions.json")?;

        Ok(Some(state))
    }

    /// Save session state to ~/.wt/sessions.json
    pub fn save(&self) -> Result<()> {
        let path = Self::state_file_path()?;
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

    /// Sync session state with actual tmux windows
    /// Removes entries for windows that no longer exist
    pub fn sync_with_tmux(&mut self, tmux: &TmuxManager) -> Result<()> {
        let windows = tmux.list_windows()?;
        let window_names: std::collections::HashSet<_> =
            windows.iter().map(|w| w.name.clone()).collect();

        // Remove worktrees that no longer have windows
        self.worktrees
            .retain(|name, _| window_names.contains(name));

        // Update pane counts
        for window in &windows {
            if let Some(info) = self.worktrees.get_mut(&window.name) {
                info.pane_count = window.pane_count as u8;
            }
        }

        Ok(())
    }

    /// Clear the session state
    pub fn clear() -> Result<()> {
        let path = Self::state_file_path()?;
        if path.exists() {
            std::fs::remove_file(&path).context("Failed to remove sessions.json")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_state_new() {
        let state = SessionState::new("wt");
        assert_eq!(state.session_name, "wt");
        assert!(state.worktrees.is_empty());
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
}
