use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

use wt::config::{Config, SessionConfig};
use wt::session::SessionState;
use wt::tmux_manager::TmuxManager;

fn setup_test_repo() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let repo_path = temp_dir.path().to_path_buf();

    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(&repo_path)
        .output()
        .expect("Failed to init git repo");

    Command::new("git")
        .args(["commit", "--allow-empty", "-m", "init"])
        .current_dir(&repo_path)
        .output()
        .expect("Failed to create initial commit");

    (temp_dir, repo_path)
}

#[test]
#[ignore]
fn test_tmux_session_lifecycle() {
    if !TmuxManager::is_available() {
        eprintln!("tmux not available, skipping test");
        return;
    }

    let session_name = "wt-test-session";
    let tmux = TmuxManager::new(session_name);
    let (_temp_dir, repo_path) = setup_test_repo();

    // Cleanup any existing test session
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .output();

    // Test session creation
    assert!(!tmux.session_exists().unwrap());
    tmux.create_session("test-window", &repo_path).unwrap();
    assert!(tmux.session_exists().unwrap());

    // Test window listing
    let windows = tmux.list_windows().unwrap();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].name, "test-window");

    // Test window creation
    tmux.create_window("second-window", &repo_path).unwrap();
    let windows = tmux.list_windows().unwrap();
    assert_eq!(windows.len(), 2);

    // Test window removal
    tmux.kill_window("second-window").unwrap();
    let windows = tmux.list_windows().unwrap();
    assert_eq!(windows.len(), 1);

    // Cleanup
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .output();
}

#[test]
#[ignore]
fn test_tmux_pane_layout_2_panes() {
    if !TmuxManager::is_available() {
        eprintln!("tmux not available, skipping test");
        return;
    }

    let session_name = "wt-test-layout-2";
    let tmux = TmuxManager::new(session_name);
    let (_temp_dir, repo_path) = setup_test_repo();

    // Cleanup any existing test session
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .output();

    let config = SessionConfig::default();
    tmux.create_session("test-window", &repo_path).unwrap();
    tmux.setup_worktree_layout("test-window", &repo_path, 2, &config)
        .unwrap();

    let windows = tmux.list_windows().unwrap();
    assert_eq!(windows[0].pane_count, 2);

    // Cleanup
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .output();
}

#[test]
#[ignore]
fn test_tmux_pane_layout_3_panes() {
    if !TmuxManager::is_available() {
        eprintln!("tmux not available, skipping test");
        return;
    }

    let session_name = "wt-test-layout-3";
    let tmux = TmuxManager::new(session_name);
    let (_temp_dir, repo_path) = setup_test_repo();

    // Cleanup any existing test session
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .output();

    let config = SessionConfig::default();
    tmux.create_session("test-window", &repo_path).unwrap();
    tmux.setup_worktree_layout("test-window", &repo_path, 3, &config)
        .unwrap();

    let windows = tmux.list_windows().unwrap();
    assert_eq!(windows[0].pane_count, 3);

    // Cleanup
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .output();
}

#[test]
fn test_session_state_persistence() {
    let mut state = SessionState::new("test-session");
    state.add_worktree("feature-1", 0, 2, PathBuf::from("/tmp/feature-1"));
    state.add_worktree("feature-2", 1, 3, PathBuf::from("/tmp/feature-2"));

    let json = serde_json::to_string(&state).unwrap();
    let loaded: SessionState = serde_json::from_str(&json).unwrap();

    assert_eq!(loaded.session_name, "test-session");
    assert!(loaded.has_worktree("feature-1"));
    assert!(loaded.has_worktree("feature-2"));

    let info = loaded.get_worktree("feature-1").unwrap();
    assert_eq!(info.pane_count, 2);

    let info = loaded.get_worktree("feature-2").unwrap();
    assert_eq!(info.pane_count, 3);
}

#[test]
fn test_config_defaults() {
    let config = Config::default();
    assert_eq!(config.session.panes, 2);
    assert_eq!(config.session.agent_cmd, "claude");
    assert_eq!(config.session.editor_cmd, "nvim");
}

#[test]
fn test_config_effective_panes() {
    let config = Config::default();

    // No override - use default
    assert_eq!(config.effective_panes(None), 2);

    // Override with valid values
    assert_eq!(config.effective_panes(Some(2)), 2);
    assert_eq!(config.effective_panes(Some(3)), 3);

    // Override clamped to valid range
    assert_eq!(config.effective_panes(Some(1)), 2);
    assert_eq!(config.effective_panes(Some(4)), 3);
}
