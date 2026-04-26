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
const NO_WINDOWS_SESSIONS_MSG: &str =
    "No worktree sessions found. Use 'wt session add <name>' to create one.";

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SessionRmProbe {
    worktree_exists: bool,
    panes_has_worktree: bool,
    windows_session_name: String,
    windows_session_tracked: bool,
    windows_session_live: bool,
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
            SessionMode::Panes => {
                let tmux = panes_tmux();
                cmd_session_attach(&tmux)
            }
            SessionMode::Windows => cmd_session_attach_windows(),
        },
        Some(SessionAction::Ls) => match context.mode {
            SessionMode::Panes => {
                let tmux = panes_tmux();
                cmd_session_ls(&tmux)
            }
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
            SessionMode::Panes => cmd_session_rm_panes(&context, &name),
            SessionMode::Windows => cmd_session_rm_windows(&context, &name),
        },
        Some(SessionAction::Watch { interval }) => match context.mode {
            SessionMode::Panes => {
                let tmux = panes_tmux();
                cmd_session_watch(&tmux, interval)
            }
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
            manager.create_worktree(name, base, &context.repo.worktree_dir, |remotes| {
                choose_remote_branch(name, remotes)
            })
        }
    }
}

fn choose_remote_branch(name: &str, remotes: &[String]) -> Result<String> {
    if remotes.is_empty() {
        anyhow::bail!("No remote branches match '{}'.", name);
    }

    if remotes.len() == 1 {
        return Ok(remotes[0].clone());
    }

    let selection = Select::new()
        .with_prompt(format!("Select remote branch for '{}'", name))
        .items(remotes)
        .default(0)
        .interact()?;

    Ok(remotes[selection].clone())
}

fn panes_tmux() -> TmuxManager {
    TmuxManager::new(SESSION_NAME)
}

fn create_status_window_session(tmux: &TmuxManager, repo_root: &Path) -> Result<()> {
    tmux.create_session("status", repo_root)?;
    tmux.send_keys("status", 0, "wt session watch")?;
    Ok(())
}

fn ensure_status_window(tmux: &TmuxManager, repo_root: &Path) -> Result<()> {
    if tmux
        .list_windows()?
        .iter()
        .any(|window| window.name == "status")
    {
        return Ok(());
    }

    tmux.create_window("status", repo_root)?;
    tmux.send_keys("status", 0, "wt session watch")?;
    Ok(())
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
    let tmux = panes_tmux();
    let worktree_path = ensure_worktree_path(context, name, base)?;
    let panes = context.effective_panes(panes_override);
    let inside_session = tmux.is_inside_session();

    if !tmux.session_exists()? {
        eprintln!("Creating tmux session: {}", SESSION_NAME);
        if watch {
            create_status_window_session(&tmux, &context.repo.root)?;
            tmux.create_window(name, &worktree_path)?;
        } else {
            tmux.create_session(name, &worktree_path)?;
        }
        tmux.setup_worktree_layout(name, &worktree_path, panes, &context.config.session)?;
    } else {
        if watch {
            ensure_status_window(&tmux, &context.repo.root)?;
        }

        let windows = tmux.list_windows()?;

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

fn cmd_session_rm_panes(context: &SessionCmdContext<'_>, name: &str) -> Result<()> {
    let tmux = panes_tmux();

    if !tmux.session_exists()? {
        eprintln!("No session found.");
        print_rm_hint(SessionMode::Panes, name, &probe_session_rm(context, name)?);
        return Ok(());
    }

    let windows = tmux.list_windows()?;
    if !windows.iter().any(|window| window.name == name) {
        eprintln!("Window '{}' not found in session.", name);
        print_rm_hint(SessionMode::Panes, name, &probe_session_rm(context, name)?);
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
            state.sync_with_tmux(&tmux)?;
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
    let Some(state) = load_windows_state_or_report_empty()? else {
        return Ok(());
    };

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
    let Some(state) = load_windows_state_or_report_empty()? else {
        return Ok(());
    };

    for (_, info) in sorted_windows_sessions(&state) {
        let tmux = TmuxManager::new(&info.session_name);
        let attached = tmux.is_attached().unwrap_or(false);
        let agent_status = agent_window_status(&tmux);
        let marker = if attached { "*" } else { " " };
        println!("{} {} (agent: {})", marker, info.session_name, agent_status);
    }

    Ok(())
}

fn cmd_session_rm_windows(context: &SessionCmdContext<'_>, name: &str) -> Result<()> {
    let probe = probe_session_rm(context, name)?;
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
    }

    if let Some(loaded) = state.as_mut() {
        let removed = loaded.remove_windows_session(name).is_some();
        prune_windows_state(loaded);
        save_state_or_clear_if_empty(loaded)?;
        if removed && !session_existed {
            eprintln!(
                "Removed stale windows-mode entry for '{}' (session '{}').",
                name, session_name
            );
            if probe.panes_has_worktree {
                print_rm_hint(SessionMode::Windows, name, &probe);
            } else if probe.worktree_exists {
                eprintln!(
                    "Worktree '{}' still exists. Use 'wt rm {}' to remove the \
                     worktree or 'wt session --mode windows add {}' to add it \
                     again.",
                    name, name, name
                );
            }
            return Ok(());
        }
    }

    if !session_existed {
        eprintln!("Session '{}' not found.", session_name);
        print_rm_hint(SessionMode::Windows, name, &probe);
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

    state.add_windows_session(
        worktree_name,
        WindowsSessionInfo {
            session_name: session_name.to_string(),
            worktree_path: worktree_path.to_path_buf(),
            windows: windows_layout_names(panes),
        },
    );
    prune_windows_state(&mut state);
    state.save()
}

fn load_windows_state() -> Result<Option<SessionState>> {
    let Some(mut state) = SessionState::load()? else {
        return Ok(None);
    };

    prune_windows_state(&mut state);
    save_state_or_clear_if_empty(&state)?;
    Ok(Some(state))
}

fn load_windows_state_or_report_empty() -> Result<Option<SessionState>> {
    let Some(state) = load_windows_state()? else {
        eprintln!("{}", NO_WINDOWS_SESSIONS_MSG);
        return Ok(None);
    };

    if state.windows_sessions.is_empty() {
        eprintln!("{}", NO_WINDOWS_SESSIONS_MSG);
        return Ok(None);
    }

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

fn windows_layout_names(panes: u8) -> Vec<String> {
    if panes == 3 {
        vec!["agent".to_string(), "shell".to_string(), "edit".to_string()]
    } else {
        vec!["agent".to_string(), "shell".to_string()]
    }
}

fn agent_window_status(tmux: &TmuxManager) -> AgentStatus {
    tmux.list_windows()
        .ok()
        .and_then(|windows| windows.into_iter().find(|window| window.name == "agent"))
        .map(|window| window.agent_status)
        .unwrap_or(AgentStatus::Unknown)
}

fn probe_session_rm(context: &SessionCmdContext<'_>, name: &str) -> Result<SessionRmProbe> {
    let manager = WorktreeManager::new(context.repo.root.clone())?;
    let panes_tmux = TmuxManager::new(SESSION_NAME);
    let panes_has_worktree = if panes_tmux.session_exists()? {
        panes_tmux
            .list_windows()?
            .into_iter()
            .any(|window| window.name == name)
    } else {
        false
    };

    let state = SessionState::load()?;
    let tracked_windows_session_name = state
        .as_ref()
        .and_then(|loaded| loaded.windows_sessions.get(name))
        .map(|info| info.session_name.clone());
    let windows_session_tracked = tracked_windows_session_name.is_some();
    let windows_session_name = tracked_windows_session_name
        .unwrap_or_else(|| context.config.session.session_name_for(name));
    let windows_session_live = TmuxManager::new(&windows_session_name).session_exists()?;

    Ok(SessionRmProbe {
        worktree_exists: manager.worktree_exists(name),
        panes_has_worktree,
        windows_session_name,
        windows_session_tracked,
        windows_session_live,
    })
}

fn panes_rm_hint(name: &str, probe: &SessionRmProbe) -> Option<String> {
    if probe.windows_session_live {
        let availability = if probe.windows_session_tracked {
            "tracked in windows mode"
        } else {
            "available in windows mode"
        };
        Some(format!(
            "'{}' is {} as session '{}'. Try: wt session --mode windows rm {}",
            name, availability, probe.windows_session_name, name
        ))
    } else if probe.worktree_exists {
        Some(format!(
            "Worktree '{}' exists but is not in the shared panes session. \
             Use 'wt rm {}' to remove the worktree or 'wt session --mode \
             panes add {}' to add it.",
            name, name, name
        ))
    } else {
        None
    }
}

fn windows_rm_hint(name: &str, probe: &SessionRmProbe) -> Option<String> {
    if probe.panes_has_worktree {
        Some(format!(
            "'{}' is in the shared panes session. Try: wt session --mode \
             panes rm {}",
            name, name
        ))
    } else if probe.worktree_exists && !probe.windows_session_tracked {
        Some(format!(
            "Worktree '{}' exists but is not tracked in windows mode. Use \
             'wt rm {}' to remove the worktree or 'wt session --mode \
             windows add {}' to add it.",
            name, name, name
        ))
    } else {
        None
    }
}

fn print_rm_hint(mode: SessionMode, name: &str, probe: &SessionRmProbe) {
    let hint = match mode {
        SessionMode::Panes => panes_rm_hint(name, probe),
        SessionMode::Windows => windows_rm_hint(name, probe),
    };

    if let Some(hint) = hint {
        eprintln!("{}", hint);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe() -> SessionRmProbe {
        SessionRmProbe {
            windows_session_name: "wt-demo".to_string(),
            ..SessionRmProbe::default()
        }
    }

    #[test]
    fn test_rm_hint_points_panes_removal_to_windows_mode() {
        let mut probe = probe();
        probe.windows_session_tracked = true;
        probe.windows_session_live = true;

        assert_eq!(
            panes_rm_hint("demo", &probe),
            Some(
                "'demo' is tracked in windows mode as session 'wt-demo'. Try: wt session --mode windows rm demo"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_rm_hint_points_windows_removal_to_panes_mode() {
        let mut probe = probe();
        probe.panes_has_worktree = true;

        assert_eq!(
            windows_rm_hint("demo", &probe),
            Some(
                "'demo' is in the shared panes session. Try: wt session --mode panes rm demo"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_rm_hint_explains_untracked_windows_worktree() {
        let mut probe = probe();
        probe.worktree_exists = true;

        assert_eq!(
            windows_rm_hint("demo", &probe),
            Some(
                "Worktree 'demo' exists but is not tracked in windows mode. Use 'wt rm demo' to remove the worktree or 'wt session --mode windows add demo' to add it."
                    .to_string()
            )
        );
    }

    #[test]
    fn test_rm_hint_explains_missing_panes_membership() {
        let mut probe = probe();
        probe.worktree_exists = true;

        assert_eq!(
            panes_rm_hint("demo", &probe),
            Some(
                "Worktree 'demo' exists but is not in the shared panes session. Use 'wt rm demo' to remove the worktree or 'wt session --mode panes add demo' to add it."
                    .to_string()
            )
        );
    }

    #[test]
    fn test_rm_hint_is_empty_when_nothing_matches() {
        assert_eq!(panes_rm_hint("demo", &probe()), None);
        assert_eq!(windows_rm_hint("demo", &probe()), None);
    }

    #[test]
    fn test_windows_layout_names_match_pane_count() {
        assert_eq!(
            windows_layout_names(2),
            vec!["agent".to_string(), "shell".to_string()]
        );
        assert_eq!(
            windows_layout_names(3),
            vec!["agent".to_string(), "shell".to_string(), "edit".to_string()]
        );
    }
}
