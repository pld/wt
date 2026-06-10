mod session_cmd;

use anyhow::{Context, Result};
use clap::parser::ValueSource;
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use dialoguer::Select;
use std::path::{Path, PathBuf};
use std::process::Command;

use session_cmd::{run_session, SessionAction};
use wt::config::SessionMode;
use wt::shell::spawn_wt_shell;
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

#[derive(Debug, Clone)]
pub(crate) enum RepoLayout {
    Normal {
        root: PathBuf,
    },
    Bare {
        git_dir: PathBuf,
        has_worktree: bool,
    },
}

struct RepoConfig {
    layout: RepoLayout,
    worktree_dir: PathBuf,
    git_cwd: PathBuf,
}

impl RepoConfig {
    fn new(dir_arg: &Path, dir_explicit: bool) -> Result<Self> {
        let layout = resolve_layout()?;
        let (git_cwd, worktree_dir) = match &layout {
            RepoLayout::Normal { root } => (root.clone(), root.join(dir_arg)),
            RepoLayout::Bare { git_dir, .. } => {
                let parent = git_dir
                    .parent()
                    .ok_or_else(|| anyhow::anyhow!("Bare git dir has no parent: {:?}", git_dir))?
                    .to_path_buf();
                let worktree_dir = if dir_explicit {
                    if dir_arg.is_absolute() {
                        dir_arg.to_path_buf()
                    } else {
                        parent.join(dir_arg)
                    }
                } else {
                    parent
                };
                (git_dir.clone(), worktree_dir)
            }
        };
        Ok(Self {
            layout,
            worktree_dir,
            git_cwd,
        })
    }

    fn is_bare(&self) -> bool {
        matches!(self.layout, RepoLayout::Bare { .. })
    }

    fn has_worktree(&self) -> bool {
        match &self.layout {
            RepoLayout::Normal { .. } => true,
            RepoLayout::Bare { has_worktree, .. } => *has_worktree,
        }
    }

    fn session_cwd(&self) -> &Path {
        match &self.layout {
            RepoLayout::Normal { root } => root,
            RepoLayout::Bare { .. } => &self.worktree_dir,
        }
    }

    fn status_cwd(&self) -> &Path {
        match &self.layout {
            RepoLayout::Normal { .. } => self.session_cwd(),
            RepoLayout::Bare { .. } => &self.git_cwd,
        }
    }
}

fn resolve_layout() -> Result<RepoLayout> {
    let is_bare = git_rev_parse(&["--is-bare-repository"])?;
    let git_common_dir = git_rev_parse(&["--git-common-dir"])?;
    let git_common_path = std::env::current_dir()?
        .join(&git_common_dir)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&git_common_dir));

    if is_bare == "true" {
        return Ok(RepoLayout::Bare {
            git_dir: git_common_path,
            has_worktree: false,
        });
    }

    // Detect being inside a linked worktree of a bare repo: the common-dir's
    // own `core.bare` is true.
    let common_is_bare = Command::new("git")
        .args(["config", "--bool", "core.bare"])
        .current_dir(&git_common_path)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true")
        .unwrap_or(false);

    if common_is_bare {
        return Ok(RepoLayout::Bare {
            git_dir: git_common_path,
            has_worktree: true,
        });
    }

    let root = get_repo_root()?;
    Ok(RepoLayout::Normal { root })
}

fn git_rev_parse(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("rev-parse")
        .args(args)
        .output()
        .context("Failed to execute git rev-parse")?;
    if !output.status.success() {
        anyhow::bail!("Not a git repository");
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
        /// Override session layout mode for this invocation
        #[arg(long, value_enum)]
        mode: Option<SessionMode>,
        #[command(subcommand)]
        action: Option<SessionAction>,
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
    let matches = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&matches)?;
    let dir_explicit = matches!(
        matches.value_source("dir"),
        Some(ValueSource::CommandLine) | Some(ValueSource::EnvVariable)
    );
    let config = RepoConfig::new(&cli.dir, dir_explicit)?;

    match cli.command {
        Commands::New {
            name,
            b,
            print_path,
        } => cmd_new(&config, name, &b, print_path),
        Commands::Use { name } => cmd_use(&config, name),
        Commands::Ls => cmd_ls(&config),
        Commands::Rm { name } => cmd_rm(&config, name),
        Commands::Which => cmd_which(&std::env::current_dir()?),
        Commands::Session { mode, action } => run_session(&config, mode, action),
    }
}

fn cmd_new(config: &RepoConfig, name: Option<String>, base: &str, print_path: bool) -> Result<()> {
    if !config.is_bare() {
        check_not_in_worktree(&std::env::current_dir()?)?;
    }

    let root_branch = get_root_branch();

    let name = match name {
        Some(n) => n,
        None => {
            if config.has_worktree() {
                let current_branch = get_current_branch()?;
                if current_branch == root_branch {
                    anyhow::bail!(
                        "On root branch '{}'. Specify a name: wt new <name>",
                        root_branch
                    );
                }
                current_branch
            } else {
                root_branch.clone()
            }
        }
    };

    let manager = WorktreeManager::new(config.git_cwd.clone())?;

    if let Some(info) = manager.get_worktree_info(&name)? {
        eprintln!(
            "Worktree '{}' already exists at {}, entering it.",
            name,
            info.path.display()
        );
        if print_path {
            println!("{}", info.path.display());
        } else {
            spawn_wt_shell(&info.path, &info.task_id, &info.branch)?;
        }
        return Ok(());
    }

    // Migration only makes sense in Normal layout (there is a working tree to migrate from).
    let had_changes = if config.is_bare() {
        false
    } else {
        let current_branch = get_current_branch()?;
        let migrating = name == current_branch && current_branch != root_branch;
        if migrating {
            migrate_from_current_branch(&config.git_cwd, &root_branch)?
        } else {
            false
        }
    };

    if !config.is_bare() {
        ensure_worktrees_in_gitignore(&config.git_cwd, &config.worktree_dir)?;
        std::fs::create_dir_all(&config.worktree_dir)?;
    }
    let path = manager.create_worktree(&name, base, &config.worktree_dir, |remotes| {
        choose_remote_branch(&name, remotes)
    })?;

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
    let manager = WorktreeManager::new(config.git_cwd.clone())?;
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
            let manager = WorktreeManager::new(config.git_cwd.clone())?;
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

    let manager = WorktreeManager::new(config.git_cwd.clone())?;
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
    let manager = WorktreeManager::new(config.git_cwd.clone())?;
    let worktrees = manager.list_worktrees()?;

    let wt_name = match name {
        Some(n) => n,
        None => {
            let current = get_current_worktree_name(&std::env::current_dir()?)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_session_add_without_agent_cmd_leaves_override_unset() {
        let cli = Cli::try_parse_from(["wt", "session", "add", "demo"])
            .expect("session add should parse without --agent-cmd");

        match cli.command {
            Commands::Session {
                action: Some(SessionAction::Add { agent_cmd, .. }),
                ..
            } => {
                assert_eq!(agent_cmd, None);
            }
            _ => panic!("expected session add command"),
        }
    }

    #[test]
    fn test_cli_parses_session_add_agent_cmd() {
        let cli = Cli::try_parse_from([
            "wt",
            "session",
            "--mode",
            "windows",
            "add",
            "demo",
            "--agent-cmd",
            "aider --fast",
        ])
        .expect("session add should accept --agent-cmd");

        match cli.command {
            Commands::Session {
                mode,
                action:
                    Some(SessionAction::Add {
                        name,
                        base,
                        panes,
                        agent_cmd,
                        watch,
                    }),
            } => {
                assert_eq!(mode, Some(SessionMode::Windows));
                assert_eq!(name, "demo");
                assert_eq!(base, "main");
                assert_eq!(panes, None);
                assert_eq!(agent_cmd.as_deref(), Some("aider --fast"));
                assert!(!watch);
            }
            _ => panic!("expected session add command"),
        }
    }
}
