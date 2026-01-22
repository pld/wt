use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dialoguer::Select;
use std::path::{Path, PathBuf};
use std::process::Command;

use wt::config::Config;
use wt::session::SessionState;
use wt::shell::spawn_wt_shell;
use wt::tmux_manager::TmuxManager;
use wt::worktree_manager::{
    check_not_in_worktree, ensure_worktrees_in_gitignore, get_current_worktree_name,
    WorktreeManager,
};

#[derive(Parser)]
#[command(
    name = "wt",
    version,
    about = "Parallel workspaces for agent sandboxes"
)]
struct Cli {
    /// Worktree directory (relative to repo root)
    #[arg(short = 'd', long, global = true, default_value = ".worktrees")]
    dir: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

struct RepoConfig {
    root: PathBuf,
    worktree_dir: PathBuf,
}

impl RepoConfig {
    fn new(dir: &Path) -> Result<Self> {
        let root = get_repo_root()?;
        let worktree_dir = root.join(dir);
        Ok(Self { root, worktree_dir })
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new workspace and enter subshell
    New {
        /// Name for the workspace (defaults to current branch, fails on root branch)
        name: Option<String>,
        /// Base branch to create from
        #[arg(short, default_value = "main")]
        b: String,
        /// Print path instead of entering shell (for scripts/agents)
        #[arg(long)]
        print_path: bool,
    },
    /// Enter an existing workspace subshell
    Use {
        /// Name of the workspace (optional if already in worktree)
        name: Option<String>,
    },
    /// List all workspaces (interactive picker)
    Ls,
    /// Remove a workspace
    Rm {
        /// Name of the workspace to remove (interactive if omitted)
        name: Option<String>,
    },
    /// Print current worktree name (or "main" if in main worktree)
    Which,
    /// Manage tmux session with multiple worktree windows
    Session {
        #[command(subcommand)]
        action: Option<SessionAction>,
    },
}

#[derive(Subcommand)]
enum SessionAction {
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

fn get_repo_root() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("Failed to execute git rev-parse")?;

    if !output.status.success() {
        anyhow::bail!("Not a git repository");
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(PathBuf::from(path))
}

fn get_current_branch() -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .context("Failed to get current branch")?;

    if !output.status.success() {
        anyhow::bail!("Failed to determine current branch");
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn get_root_branch() -> String {
    // Try to get the default branch from remote
    if let Ok(output) = Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .output()
    {
        if output.status.success() {
            let refname = String::from_utf8_lossy(&output.stdout);
            if let Some(branch) = refname.trim().strip_prefix("refs/remotes/origin/") {
                return branch.to_string();
            }
        }
    }

    // Fall back to checking if main or master exists
    for branch in ["main", "master"] {
        if Command::new("git")
            .args(["rev-parse", "--verify", branch])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return branch.to_string();
        }
    }

    "main".to_string()
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = RepoConfig::new(&cli.dir)?;

    match cli.command {
        Commands::New {
            name,
            b,
            print_path,
        } => cmd_new(&config, name, &b, print_path),
        Commands::Use { name } => cmd_use(&config, name),
        Commands::Ls => cmd_ls(&config),
        Commands::Rm { name } => cmd_rm(&config, name),
        Commands::Which => cmd_which(&config.root),
        Commands::Session { action } => cmd_session(&config, action),
    }
}

fn cmd_new(config: &RepoConfig, name: Option<String>, base: &str, print_path: bool) -> Result<()> {
    check_not_in_worktree(&config.root)?;

    let current_branch = get_current_branch()?;
    let root_branch = get_root_branch();

    let name = match name {
        Some(n) => n,
        None => {
            if current_branch == root_branch {
                anyhow::bail!(
                    "On root branch '{}'. Specify a name: wt new <name>",
                    root_branch
                );
            }
            current_branch.clone()
        }
    };

    // If creating worktree for currently checked out branch, migrate the work
    let migrating = name == current_branch && current_branch != root_branch;
    let had_changes = if migrating {
        migrate_from_current_branch(&config.root, &root_branch)?
    } else {
        false
    };

    let manager = WorktreeManager::new(config.root.clone())?;
    ensure_worktrees_in_gitignore(&config.root, &config.worktree_dir)?;
    std::fs::create_dir_all(&config.worktree_dir)?;
    let path = manager.create_worktree(&name, base, &config.worktree_dir)?;

    // Pop stash in the new worktree if we migrated changes
    if had_changes {
        let output = Command::new("git")
            .args(["stash", "pop"])
            .current_dir(&path)
            .output()
            .context("Failed to pop stash")?;
        if !output.status.success() {
            eprintln!(
                "Warning: Failed to restore changes: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    if print_path {
        println!("{}", path.display());
    } else {
        spawn_wt_shell(&path, &name, &name)?;
    }
    Ok(())
}

fn migrate_from_current_branch(repo_path: &Path, root_branch: &str) -> Result<bool> {
    // Check for uncommitted changes
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_path)
        .output()
        .context("Failed to check git status")?;

    let has_changes = !status.stdout.is_empty();

    if has_changes {
        eprintln!("Stashing uncommitted changes...");
        let stash = Command::new("git")
            .args(["stash", "push", "-m", "wt: migrating to worktree"])
            .current_dir(repo_path)
            .output()
            .context("Failed to stash changes")?;
        if !stash.status.success() {
            anyhow::bail!(
                "Failed to stash changes: {}",
                String::from_utf8_lossy(&stash.stderr)
            );
        }
    }

    eprintln!("Switching to {}...", root_branch);
    let checkout = Command::new("git")
        .args(["checkout", root_branch])
        .current_dir(repo_path)
        .output()
        .context("Failed to switch branches")?;

    if !checkout.status.success() {
        // Try to restore stash if checkout failed
        if has_changes {
            let _ = Command::new("git")
                .args(["stash", "pop"])
                .current_dir(repo_path)
                .output();
        }
        anyhow::bail!(
            "Failed to switch to {}: {}",
            root_branch,
            String::from_utf8_lossy(&checkout.stderr)
        );
    }

    Ok(has_changes)
}

enum PickResult {
    Selected(String),
    ExitShell,
    Cancelled,
    Empty,
}

fn pick_worktree(config: &RepoConfig, prompt: &str) -> Result<PickResult> {
    let manager = WorktreeManager::new(config.root.clone())?;
    let worktrees = manager.list_worktrees()?;

    let in_wt_shell = std::env::var("WT_ACTIVE").is_ok();
    let current_wt = std::env::var("WT_NAME").ok();

    let wt_list: Vec<_> = worktrees
        .iter()
        .filter(|wt| !wt.task_id.is_empty())
        .collect();

    if wt_list.is_empty() {
        return Ok(PickResult::Empty);
    }

    // Non-interactive mode if not a TTY
    if !std::io::IsTerminal::is_terminal(&std::io::stderr()) {
        for wt in &wt_list {
            let marker = if Some(&wt.task_id) == current_wt.as_ref() {
                " *"
            } else {
                ""
            };
            println!("{}{}", wt.task_id, marker);
        }
        return Ok(PickResult::Cancelled);
    }

    let mut items: Vec<String> = wt_list
        .iter()
        .map(|wt| {
            let marker = if Some(&wt.task_id) == current_wt.as_ref() {
                " *"
            } else {
                ""
            };
            format!("{}{}", wt.task_id, marker)
        })
        .collect();

    // Always add cancel/exit option
    if in_wt_shell {
        items.push("← exit shell".to_string());
    } else {
        items.push("← cancel".to_string());
    }

    let default = if let Some(ref name) = current_wt {
        items.iter().position(|i| i.starts_with(name)).unwrap_or(0)
    } else {
        0
    };

    eprintln!("{}", prompt);
    let selection = Select::new().items(&items).default(default).interact()?;

    let selected = &items[selection];

    if selected == "← exit shell" {
        return Ok(PickResult::ExitShell);
    }

    if selected == "← cancel" {
        return Ok(PickResult::Cancelled);
    }

    let wt_name = selected.trim_end_matches(" *").to_string();
    Ok(PickResult::Selected(wt_name))
}

fn cmd_ls(config: &RepoConfig) -> Result<()> {
    match pick_worktree(config, "Select worktree:")? {
        PickResult::Empty => {
            eprintln!("No worktrees found.");
        }
        PickResult::ExitShell => {
            eprintln!("Type 'exit' to leave this worktree shell.");
        }
        PickResult::Cancelled => {}
        PickResult::Selected(name) => {
            let manager = WorktreeManager::new(config.root.clone())?;
            let wt_info = manager
                .get_worktree_info(&name)?
                .ok_or_else(|| anyhow::anyhow!("Worktree not found"))?;
            spawn_wt_shell(&wt_info.path, &wt_info.task_id, &wt_info.branch)?;
        }
    }
    Ok(())
}

fn cmd_rm(config: &RepoConfig, name: Option<String>) -> Result<()> {
    let name = match name {
        Some(n) => n,
        None => match pick_worktree(config, "Remove worktree:")? {
            PickResult::Selected(n) => n,
            PickResult::Empty => {
                eprintln!("No worktrees found.");
                return Ok(());
            }
            _ => return Ok(()),
        },
    };

    let manager = WorktreeManager::new(config.root.clone())?;
    manager.remove_worktree(&name)?;
    eprintln!("Removed worktree: {}", name);
    Ok(())
}

fn cmd_which(repo_path: &Path) -> Result<()> {
    let name = get_current_worktree_name(repo_path)?;
    println!("{}", name);
    Ok(())
}

fn cmd_use(config: &RepoConfig, name: Option<String>) -> Result<()> {
    let manager = WorktreeManager::new(config.root.clone())?;
    let worktrees = manager.list_worktrees()?;

    let wt_name = match name {
        Some(n) => n,
        None => {
            let current = get_current_worktree_name(&config.root)?;
            if current == "main" {
                anyhow::bail!("Not in a worktree. Specify a worktree name: wt use <name>");
            }
            current
        }
    };

    let wt_info = worktrees
        .iter()
        .find(|w| w.task_id == wt_name)
        .ok_or_else(|| anyhow::anyhow!("Worktree '{}' not found", wt_name))?;

    spawn_wt_shell(&wt_info.path, &wt_info.task_id, &wt_info.branch)?;
    Ok(())
}

const SESSION_NAME: &str = "wt";

fn cmd_session(config: &RepoConfig, action: Option<SessionAction>) -> Result<()> {
    if !TmuxManager::is_available() {
        eprintln!("tmux not found. Falling back to interactive picker...");
        return cmd_ls(config);
    }

    let wt_config = Config::load_for_repo(&config.root);
    let tmux = TmuxManager::new(SESSION_NAME);

    match action {
        None => cmd_session_attach(&tmux),
        Some(SessionAction::Ls) => cmd_session_ls(&tmux),
        Some(SessionAction::Add {
            name,
            base,
            panes,
            watch,
        }) => cmd_session_add(config, &tmux, &wt_config, &name, &base, panes, watch),
        Some(SessionAction::Rm { name }) => cmd_session_rm(&tmux, &name),
        Some(SessionAction::Watch { interval }) => cmd_session_watch(&tmux, interval),
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

    tmux.attach()?;
    Ok(())
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
        // Skip the status window in listing
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

fn cmd_session_add(
    config: &RepoConfig,
    tmux: &TmuxManager,
    wt_config: &Config,
    name: &str,
    base: &str,
    panes_override: Option<u8>,
    watch: bool,
) -> Result<()> {
    check_not_in_worktree(&config.root)?;

    let manager = WorktreeManager::new(config.root.clone())?;
    ensure_worktrees_in_gitignore(&config.root, &config.worktree_dir)?;
    std::fs::create_dir_all(&config.worktree_dir)?;

    // Check if worktree already exists
    let existing = manager.get_worktree_info(name)?;
    let worktree_path = if let Some(info) = existing {
        eprintln!("Using existing worktree: {}", name);
        info.path
    } else {
        eprintln!("Creating worktree: {}", name);
        manager.create_worktree(name, base, &config.worktree_dir)?
    };

    let panes = wt_config.effective_panes(panes_override);
    let inside_session = tmux.is_inside_session();

    // Create or get session
    let session_exists = tmux.session_exists()?;
    if !session_exists {
        eprintln!("Creating tmux session: {}", SESSION_NAME);
        if watch {
            // Create session with status window first
            tmux.create_session("status", &config.root)?;
            tmux.send_keys("status", 0, "wt session watch")?;
            tmux.create_window(name, &worktree_path)?;
        } else {
            // Create session with worktree as first window
            tmux.create_session(name, &worktree_path)?;
        }
        tmux.setup_worktree_layout(name, &worktree_path, panes, &wt_config.session)?;
    } else {
        let windows = tmux.list_windows()?;

        // Add status window if --watch and not present
        if watch && !windows.iter().any(|w| w.name == "status") {
            tmux.create_window("status", &config.root)?;
            tmux.send_keys("status", 0, "wt session watch")?;
        }

        // Check if worktree window already exists
        if windows.iter().any(|w| w.name == name) {
            eprintln!("Window '{}' already exists in session.", name);
            if inside_session {
                tmux.select_window(name)?;
            }
        } else {
            eprintln!("Adding window: {} ({} panes)", name, panes);
            tmux.create_window(name, &worktree_path)?;
            tmux.setup_worktree_layout(name, &worktree_path, panes, &wt_config.session)?;
        }
    }

    // Save session state
    let mut state = SessionState::load()?.unwrap_or_else(|| SessionState::new(SESSION_NAME));
    state.add_worktree(name, 0, panes, worktree_path);
    state.sync_with_tmux(tmux)?;
    state.save()?;

    if inside_session {
        // Already inside, just switch to the window
        tmux.select_window(name)?;
    } else {
        eprintln!("Attaching to session...");
        tmux.attach()?;
    }

    Ok(())
}

fn cmd_session_rm(tmux: &TmuxManager, name: &str) -> Result<()> {
    if !tmux.session_exists()? {
        eprintln!("No session found.");
        return Ok(());
    }

    let windows = tmux.list_windows()?;
    if !windows.iter().any(|w| w.name == name) {
        eprintln!("Window '{}' not found in session.", name);
        return Ok(());
    }

    tmux.kill_window(name)?;
    eprintln!("Removed window: {}", name);

    // Update session state
    if let Some(mut state) = SessionState::load()? {
        state.remove_worktree(name);
        state.sync_with_tmux(tmux)?;
        state.save()?;
    }

    // Check if session is now empty (excluding status window)
    let remaining: Vec<_> = tmux
        .list_windows()?
        .into_iter()
        .filter(|w| w.name != "status")
        .collect();
    if remaining.is_empty() {
        eprintln!("Session is empty.");
        SessionState::clear()?;
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
        // Clear screen and move cursor to top
        print!("\x1B[2J\x1B[H");
        std::io::stdout().flush()?;

        println!("wt session status (refresh: {}s)\n", interval);

        let windows = tmux.list_windows()?;
        let worktrees: Vec<_> = windows.iter().filter(|w| w.name != "status").collect();

        if worktrees.is_empty() {
            println!("  No worktrees in session.");
        } else {
            for window in &worktrees {
                let status_icon = match window.agent_status {
                    wt::tmux_manager::AgentStatus::Active => "\x1B[32m●\x1B[0m", // green dot
                    wt::tmux_manager::AgentStatus::Idle => "\x1B[90m○\x1B[0m",   // gray circle
                    wt::tmux_manager::AgentStatus::Unknown => "\x1B[33m?\x1B[0m", // yellow ?
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
