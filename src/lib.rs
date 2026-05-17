pub mod config;
pub mod session;
pub mod shell;
pub mod tmux_manager;
pub mod worktree_manager;

/// Serialises tests that mutate process-global env vars (XDG_*, HOME).
/// Tests run in parallel by default; grabbing this lock prevents races on
/// std::env between any two tests in the lib crate.
#[cfg(test)]
pub(crate) static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());
