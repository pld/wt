use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dialoguer::Select;
use std::path::{Path, PathBuf};
use std::process::Command;

use wt::shell::spawn_wt_shell;
use wt::worktree_manager::{
    check_not_in_worktree, ensure_worktrees_in_gitignore, get_current_worktree_name,
    WorktreeManager,
};

#[derive(Parser)]
#[command(name = "wt", version, about = "Parallel workspaces for agent sandboxes")]
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
        Commands::New { name, b, print_path } => cmd_new(&config, name, &b, print_path),
        Commands::Use { name } => cmd_use(&config, name),
        Commands::Ls => cmd_ls(&config),
        Commands::Rm { name } => cmd_rm(&config, name),
        Commands::Which => cmd_which(&config.root),
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
        items
            .iter()
            .position(|i| i.starts_with(name))
            .unwrap_or(0)
    } else {
        0
    };

    eprintln!("{}", prompt);
    let selection = Select::new()
        .items(&items)
        .default(default)
        .interact()?;

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
            let wt_info = manager.get_worktree_info(&name)?
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

