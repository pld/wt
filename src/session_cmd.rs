use anyhow::Result;
use clap::Subcommand;
use dialoguer::Select;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::{cmd_ls, RepoConfig};
use wt::config::{Config, SessionMode};
use wt::session::{retain_live_sessions, SessionState, WindowsSessionInfo};
use wt::tmux_manager::{AgentStatus, TmuxManager};
use wt::worktree_manager::{check_not_in_worktree, ensure_worktrees_in_gitignore, WorktreeManager};

const SESSION_NAME: &str = "wt";

#[derive(Subcommand)]
pub(crate) enum SessionAction {
    /// List worktrees in the session
    Ls,
    /// Add a worktree to the session
    Add {
        /// Name for the worktree
        name: String,
        /// Base branch to create from
        #[arg(short, default_value = "main")]
        base: String,
        /// Override pane count (2 or 3)
        #[arg(long)]
        panes: Option<u8>,
        /// Create status window with live agent status
        #[arg(long)]
        watch: bool,
    },
    /// Remove a worktree from the session
    Rm {
        /// Name of the worktree to remove
        name: String,
    },
    /// Watch session status (live-updating display)
    Watch {
        /// Refresh interval in seconds
        #[arg(short, default_value = "2")]
        interval: u64,
    },
}

struct SessionCmdContext<'a> {
    repo: &'a RepoConfig,
    config: Config,
    mode: SessionMode,
}

impl<'a> SessionCmdContext<'a> {
    fn new(repo: &'a RepoConfig, mode_override: Option<SessionMode>) -> Self {
        let config = Config::load_for_repo(&repo.root);
        let mode = mode_override.unwrap_or(config.session.mode);

        Self { repo, config, mode }
    }

    fn effective_panes(&self, panes_override: Option<u8>) -> u8 {
        self.config.effective_panes(panes_override)
    }
}

pub(crate) fn run_session(
    repo: &RepoConfig,
    mode_override: Option<SessionMode>,
    action: Option<SessionAction>,
) -> Result<()> {
    if !TmuxManager::is_available() {
        eprintln!("tmux not found. Falling back to interactive picker...");
        return cmd_ls(repo);
    }

    let context = SessionCmdContext::new(repo, mode_override);

    match action {
        None => match context.mode {
            SessionMode::Panes => cmd_session_attach(&TmuxManager::new(SESSION_NAME)),
            SessionMode::Windows => cmd_session_attach_windows(),
        },
        Some(SessionAction::Ls) => match context.mode {
            SessionMode::Panes => cmd_session_ls(&TmuxManager::new(SESSION_NAME)),
            SessionMode::Windows => cmd_session_ls_windows(),
        },
        Some(SessionAction::Add {
            name,
            base,
            panes,
            watch,
        }) => match context.mode {
            SessionMode::Panes => cmd_session_add_panes(&context, &name, &base, panes, watch),
            SessionMode::Windows => cmd_session_add_windows(&context, &name, &base, panes, watch),
        },
        Some(SessionAction::Rm { name }) => match context.mode {
            SessionMode::Panes => cmd_session_rm_panes(&TmuxManager::new(SESSION_NAME), &name),
            SessionMode::Windows => cmd_session_rm_windows(&context, &name),
        },
        Some(SessionAction::Watch { interval }) => match context.mode {
            SessionMode::Panes => cmd_session_watch(&TmuxManager::new(SESSION_NAME), interval),
            SessionMode::Windows => {
                eprintln!(
                    "'wt session watch' is not yet supported in windows mode. \
                     Use 'wt session ls' to inspect status per session."
                );
                Ok(())
            }
        },
    }
}

fn ensure_worktree_path(
    context: &SessionCmdContext<'_>,
    name: &str,
    base: &str,
) -> Result<PathBuf> {
    check_not_in_worktree(&context.repo.root)?;

    let manager = WorktreeManager::new(context.repo.root.clone())?;
    ensure_worktrees_in_gitignore(&context.repo.root, &context.repo.worktree_dir)?;
    std::fs::create_dir_all(&context.repo.worktree_dir)?;

    match manager.get_worktree_info(name)? {
        Some(info) => {
            eprintln!("Using existing worktree: {}", name);
            Ok(info.path)
        }
        None => {
            eprintln!("Creating worktree: {}", name);
            manager.create_worktree(name, base, &context.repo.worktree_dir)
        }
    }
}

fn cmd_session_attach(tmux: &TmuxManager) -> Result<()> {
    if !tmux.session_exists()? {
        eprintln!("No session found. Use 'wt session add <name>' to create one.");
        return Ok(());
    }

    if tmux.is_inside_session() {
        eprintln!("Already inside session. Use 'wt session ls' to list windows.");
        return Ok(());
    }

    tmux.enter()
}

fn cmd_session_ls(tmux: &TmuxManager) -> Result<()> {
    if !tmux.session_exists()? {
        eprintln!("No session found.");
        return Ok(());
    }

    let windows = tmux.list_windows()?;
    if windows.is_empty() {
        eprintln!("No worktrees in session.");
        return Ok(());
    }

    for window in &windows {
        if window.name == "status" {
            continue;
        }

        let active_marker = if window.active { "*" } else { " " };
        println!(
            "{} [{}] {} ({}) [{} panes]",
            active_marker, window.index, window.name, window.agent_status, window.pane_count
        );
    }

    Ok(())
}

fn cmd_session_add_panes(
    context: &SessionCmdContext<'_>,
    name: &str,
    base: &str,
    panes_override: Option<u8>,
    watch: bool,
) -> Result<()> {
    let tmux = TmuxManager::new(SESSION_NAME);
    let worktree_path = ensure_worktree_path(context, name, base)?;
    let panes = context.effective_panes(panes_override);
    let inside_session = tmux.is_inside_session();

    if !tmux.session_exists()? {
        eprintln!("Creating tmux session: {}", SESSION_NAME);
        if watch {
            tmux.create_session("status", &context.repo.root)?;
            tmux.send_keys("status", 0, "wt session watch")?;
            tmux.create_window(name, &worktree_path)?;
        } else {
            tmux.create_session(name, &worktree_path)?;
        }
        tmux.setup_worktree_layout(name, &worktree_path, panes, &context.config.session)?;
    } else {
        let windows = tmux.list_windows()?;

        if watch && !windows.iter().any(|window| window.name == "status") {
            tmux.create_window("status", &context.repo.root)?;
            tmux.send_keys("status", 0, "wt session watch")?;
        }

        if windows.iter().any(|window| window.name == name) {
            eprintln!("Window '{}' already exists in session.", name);
            if inside_session {
                tmux.select_window(name)?;
            }
        } else {
            eprintln!("Adding window: {} ({} panes)", name, panes);
            tmux.create_window(name, &worktree_path)?;
            tmux.setup_worktree_layout(name, &worktree_path, panes, &context.config.session)?;
        }
    }

    let mut state = SessionState::load()?.unwrap_or_else(|| SessionState::new(SESSION_NAME));
    state.add_worktree(name, 0, panes, worktree_path);
    state.sync_with_tmux(&tmux)?;
    state.save()?;

    if inside_session {
        tmux.select_window(name)?;
    } else {
        eprintln!("Entering session...");
        tmux.enter()?;
    }

    Ok(())
}

fn cmd_session_rm_panes(tmux: &TmuxManager, name: &str) -> Result<()> {
    if !tmux.session_exists()? {
        eprintln!("No session found.");
        return Ok(());
    }

    let windows = tmux.list_windows()?;
    if !windows.iter().any(|window| window.name == name) {
        eprintln!("Window '{}' not found in session.", name);
        return Ok(());
    }

    tmux.kill_window(name)?;
    eprintln!("Removed window: {}", name);

    let remaining: Vec<_> = tmux
        .list_windows()?
        .into_iter()
        .filter(|window| window.name != "status")
        .collect();
    let session_drained = remaining.is_empty();
    if session_drained {
        eprintln!("Session is empty.");
    }

    if let Some(mut state) = SessionState::load()? {
        if session_drained {
            state.clear_panes_state();
        } else {
            state.remove_worktree(name);
            state.sync_with_tmux(tmux)?;
        }
        save_state_or_clear_if_empty(&state)?;
    }

    Ok(())
}

fn cmd_session_add_windows(
    context: &SessionCmdContext<'_>,
    name: &str,
    base: &str,
    panes_override: Option<u8>,
    watch: bool,
) -> Result<()> {
    if watch {
        eprintln!("Note: --watch is ignored in windows mode.");
    }

    let worktree_path = ensure_worktree_path(context, name, base)?;
    let panes = context.effective_panes(panes_override);
    let session_name = context.config.session.session_name_for(name);
    let tmux = TmuxManager::new(&session_name);

    if tmux.session_exists()? {
        eprintln!("Using existing session: {}", session_name);
    } else {
        eprintln!(
            "Creating tmux session: {} ({} windows)",
            session_name, panes
        );
        tmux.create_session("agent", &worktree_path)?;
        tmux.setup_worktree_windows(&worktree_path, panes, &context.config.session)?;
    }

    persist_windows_session(name, &session_name, &worktree_path, panes)?;
    tmux.enter()
}

fn cmd_session_attach_windows() -> Result<()> {
    let Some(state) = load_pruned_state()? else {
        eprintln!("No worktree sessions found. Use 'wt session add <name>' to create one.");
        return Ok(());
    };

    if state.windows_sessions.is_empty() {
        eprintln!("No worktree sessions found. Use 'wt session add <name>' to create one.");
        return Ok(());
    }

    let entries = sorted_windows_sessions(&state);
    if !std::io::stderr().is_terminal() {
        for (_, info) in &entries {
            println!("{}", info.session_name);
        }
        return Ok(());
    }

    let items: Vec<String> = entries
        .iter()
        .map(|(_, info)| info.session_name.clone())
        .chain(std::iter::once("← cancel".to_string()))
        .collect();

    eprintln!("Select worktree session:");
    let selection = Select::new().items(&items).default(0).interact()?;
    if items[selection] == "← cancel" {
        return Ok(());
    }

    TmuxManager::new(&items[selection]).enter()
}

fn cmd_session_ls_windows() -> Result<()> {
    let Some(state) = load_pruned_state()? else {
        eprintln!("No worktree sessions found. Use 'wt session add <name>' to create one.");
        return Ok(());
    };

    if state.windows_sessions.is_empty() {
        eprintln!("No worktree sessions found. Use 'wt session add <name>' to create one.");
        return Ok(());
    }

    for (_, info) in sorted_windows_sessions(&state) {
        let tmux = TmuxManager::new(&info.session_name);
        let attached = tmux.is_attached().unwrap_or(false);
        let agent_status = tmux
            .list_windows()
            .ok()
            .and_then(|windows| windows.into_iter().find(|window| window.name == "agent"))
            .map(|window| window.agent_status)
            .unwrap_or(AgentStatus::Unknown);
        let marker = if attached { "*" } else { " " };
        println!("{} {} (agent: {})", marker, info.session_name, agent_status);
    }

    Ok(())
}

fn cmd_session_rm_windows(context: &SessionCmdContext<'_>, name: &str) -> Result<()> {
    let mut state = SessionState::load()?;

    let session_name = state
        .as_ref()
        .and_then(|loaded| loaded.windows_sessions.get(name))
        .map(|info| info.session_name.clone())
        .unwrap_or_else(|| context.config.session.session_name_for(name));

    let tmux = TmuxManager::new(&session_name);
    let session_existed = tmux.session_exists()?;

    if session_existed {
        tmux.kill_session()?;
        eprintln!("Killed session: {}", session_name);
    } else {
        eprintln!("Session '{}' not found.", session_name);
    }

    if let Some(loaded) = state.as_mut() {
        let removed = loaded.remove_windows_session(name).is_some();
        prune_windows_state(loaded);
        save_state_or_clear_if_empty(loaded)?;
        if removed && !session_existed {
            eprintln!("Removed stale state entry for '{}'.", name);
        }
    }

    Ok(())
}

fn cmd_session_watch(tmux: &TmuxManager, interval: u64) -> Result<()> {
    use std::io::Write;

    if !tmux.session_exists()? {
        eprintln!("No session found.");
        return Ok(());
    }

    let interval_duration = std::time::Duration::from_secs(interval);

    loop {
        print!("\x1B[2J\x1B[H");
        std::io::stdout().flush()?;

        println!("wt session status (refresh: {}s)\n", interval);

        let windows = tmux.list_windows()?;
        let worktrees: Vec<_> = windows
            .iter()
            .filter(|window| window.name != "status")
            .collect();

        if worktrees.is_empty() {
            println!("  No worktrees in session.");
        } else {
            for window in &worktrees {
                let status_icon = match window.agent_status {
                    AgentStatus::Active => "\x1B[32m●\x1B[0m",
                    AgentStatus::Idle => "\x1B[90m○\x1B[0m",
                    AgentStatus::Unknown => "\x1B[33m?\x1B[0m",
                };
                let active_marker = if window.active { " ←" } else { "" };
                println!(
                    "  {} [{}] {}{} ({} panes)",
                    status_icon, window.index, window.name, active_marker, window.pane_count
                );
            }
        }

        println!("\n\x1B[90m● active  ○ idle  ? unknown\x1B[0m");
        println!("\x1B[90mPress Ctrl+C to exit\x1B[0m");

        std::thread::sleep(interval_duration);
    }
}

fn persist_windows_session(
    worktree_name: &str,
    session_name: &str,
    worktree_path: &Path,
    panes: u8,
) -> Result<()> {
    let mut state = SessionState::load()?.unwrap_or_else(|| SessionState::new(SESSION_NAME));

    let windows = if panes == 3 {
        vec!["agent".to_string(), "shell".to_string(), "edit".to_string()]
    } else {
        vec!["agent".to_string(), "shell".to_string()]
    };

    state.add_windows_session(
        worktree_name,
        WindowsSessionInfo {
            session_name: session_name.to_string(),
            worktree_path: worktree_path.to_path_buf(),
            windows,
        },
    );
    prune_windows_state(&mut state);
    state.save()
}

fn load_pruned_state() -> Result<Option<SessionState>> {
    let Some(mut state) = SessionState::load()? else {
        return Ok(None);
    };

    prune_windows_state(&mut state);
    save_state_or_clear_if_empty(&state)?;
    Ok(Some(state))
}

fn prune_windows_state(state: &mut SessionState) {
    if let Ok(live) = TmuxManager::live_session_names() {
        retain_live_sessions(&mut state.windows_sessions, &live);
    }
}

fn save_state_or_clear_if_empty(state: &SessionState) -> Result<()> {
    if state.is_empty() {
        SessionState::clear()
    } else {
        state.save()
    }
}

fn sorted_windows_sessions(state: &SessionState) -> Vec<(&String, &WindowsSessionInfo)> {
    let mut entries: Vec<_> = state.windows_sessions.iter().collect();
    entries.sort_by(|left, right| left.1.session_name.cmp(&right.1.session_name));
    entries
}
