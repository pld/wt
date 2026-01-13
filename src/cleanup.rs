use anyhow::Result;
use crate::worktree_manager::WorktreeManager;

#[derive(Debug, Clone, Copy)]
pub enum CleanupMode {
    Auto,
    Manual,
    KeepOnError,
}

impl CleanupMode {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(CleanupMode::Auto),
            "manual" => Some(CleanupMode::Manual),
            "keep-on-error" => Some(CleanupMode::KeepOnError),
            _ => None,
        }
    }
}

pub struct CleanupOrchestrator<'a> {
    worktree_manager: &'a WorktreeManager,
}

impl<'a> CleanupOrchestrator<'a> {
    pub fn new(worktree_manager: &'a WorktreeManager) -> Self {
        Self { worktree_manager }
    }

    pub fn cleanup_worktree(
        &self,
        task_id: &str,
        mode: CleanupMode,
        task_failed: bool,
    ) -> Result<()> {
        match mode {
            CleanupMode::Manual => Ok(()),
            CleanupMode::KeepOnError if task_failed => Ok(()),
            _ => {
                if self.worktree_manager.get_worktree_info(task_id)?.is_some() {
                    self.worktree_manager.remove_worktree(task_id)?;
                }
                Ok(())
            }
        }
    }
}
