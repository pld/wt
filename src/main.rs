use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dialoguer::Select;
use std::path::{Path, PathBuf};
use std::process::Command;

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
        /// Name of the workspace to remove
        name: String,
    },
    /// Run batch orchestration from config file
    Run {
        /// Path to tasks.yaml configuration
        config: PathBuf,
        /// Show what would be done without executing
        #[arg(long)]
        dry_run: bool,
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
        Commands::New { name, b } => cmd_new(&config, name, &b),
        Commands::Use { name } => cmd_use(&config, name),
        Commands::Ls => cmd_ls(&config),
        Commands::Rm { name } => cmd_rm(&config, &name),
        Commands::Run { config: cfg, dry_run } => wt::run::execute(&cfg, dry_run),
        Commands::Which => cmd_which(&config.root),
    }
}

fn cmd_new(config: &RepoConfig, name: Option<String>, base: &str) -> Result<()> {
    check_not_in_worktree(&config.root)?;

    let name = match name {
        Some(n) => n,
        None => {
            let current = get_current_branch()?;
            let root_branch = get_root_branch();
            if current == root_branch {
                anyhow::bail!(
                    "On root branch '{}'. Specify a name: wt new <name>",
                    root_branch
                );
            }
            current
        }
    };

    let manager = WorktreeManager::new(config.root.clone())?;
    ensure_worktrees_in_gitignore(&config.root, &config.worktree_dir)?;
    std::fs::create_dir_all(&config.worktree_dir)?;
    let path = manager.create_worktree(&name, base, &config.worktree_dir)?;
    spawn_wt_shell(&path, &name, &name)?;
    Ok(())
}

fn cmd_ls(config: &RepoConfig) -> Result<()> {
    let manager = WorktreeManager::new(config.root.clone())?;
    let worktrees = manager.list_worktrees()?;

    let in_wt_shell = std::env::var("WT_ACTIVE").is_ok();
    let current_wt = std::env::var("WT_NAME").ok();

    let wt_list: Vec<_> = worktrees
        .iter()
        .filter(|wt| !wt.task_id.is_empty())
        .collect();

    if wt_list.is_empty() {
        eprintln!("No worktrees found.");
        return Ok(());
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
        return Ok(());
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

    let selection = Select::new()
        .items(&items)
        .default(default)
        .interact()?;

    let selected = &items[selection];

    if selected == "← exit shell" {
        eprintln!("Type 'exit' to leave this worktree shell.");
        return Ok(());
    }

    if selected == "← cancel" {
        return Ok(());
    }

    let wt_name = selected.trim_end_matches(" *");
    let wt_info = wt_list.iter().find(|w| w.task_id == wt_name).unwrap();

    spawn_wt_shell(&wt_info.path, &wt_info.task_id, &wt_info.branch)?;

    Ok(())
}

fn cmd_rm(config: &RepoConfig, name: &str) -> Result<()> {
    let manager = WorktreeManager::new(config.root.clone())?;
    manager.remove_worktree(name, &config.worktree_dir)?;
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

fn spawn_wt_shell(wt_path: &Path, wt_name: &str, branch: &str) -> Result<()> {
    if std::env::var("WT_ACTIVE").is_ok() {
        anyhow::bail!("Already in a wt shell. Use 'wt ls' to switch or 'exit' first.");
    }

    let shell_path = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
    let shell_name = Path::new(&shell_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("bash");

    eprintln!("Entering worktree: {}", wt_name);

    match shell_name {
        "bash" => {
            let rcfile_content = format!(
                "[ -f ~/.bashrc ] && source ~/.bashrc; PS1=\"(wt: {}) $PS1\"",
                wt_name
            );
            let temp_rc = std::env::temp_dir().join(format!("wt-bashrc-{}", std::process::id()));
            std::fs::write(&temp_rc, rcfile_content)?;

            let result = Command::new(&shell_path)
                .arg("--rcfile")
                .arg(&temp_rc)
                .current_dir(wt_path)
                .env("WT_NAME", wt_name)
                .env("WT_BRANCH", branch)
                .env("WT_PATH", wt_path.display().to_string())
                .env("WT_ACTIVE", "1")
                .status()?;

            let _ = std::fs::remove_file(&temp_rc);
            result
        }
        "zsh" => {
            let temp_dir = create_zsh_wrapper(wt_name)?;
            let result = Command::new(&shell_path)
                .current_dir(wt_path)
                .env("ZDOTDIR", &temp_dir)
                .env("WT_NAME", wt_name)
                .env("WT_BRANCH", branch)
                .env("WT_PATH", wt_path.display().to_string())
                .env("WT_ACTIVE", "1")
                .env(
                    "_WT_ORIG_ZDOTDIR",
                    std::env::var("ZDOTDIR").unwrap_or_else(|_| std::env::var("HOME").unwrap_or_default()),
                )
                .status()?;

            let _ = std::fs::remove_dir_all(&temp_dir);
            result
        }
        "fish" => {
            Command::new(&shell_path)
                .arg("--init-command")
                .arg(format!(
                    "functions -c fish_prompt _wt_orig_prompt 2>/dev/null; \
                     function fish_prompt; echo -n '(wt: {}) '; _wt_orig_prompt; end",
                    wt_name
                ))
                .current_dir(wt_path)
                .env("WT_NAME", wt_name)
                .env("WT_BRANCH", branch)
                .env("WT_PATH", wt_path.display().to_string())
                .env("WT_ACTIVE", "1")
                .status()?
        }
        _ => {
            Command::new(&shell_path)
                .current_dir(wt_path)
                .env("WT_NAME", wt_name)
                .env("WT_BRANCH", branch)
                .env("WT_PATH", wt_path.display().to_string())
                .env("WT_ACTIVE", "1")
                .status()?
        }
    };

    show_exit_status(wt_path)?;
    Ok(())
}

fn create_zsh_wrapper(wt_name: &str) -> Result<PathBuf> {
    let temp_dir = std::env::temp_dir().join(format!("wt-zsh-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir)?;

    let zshrc_content = format!(
        r#"# Source user's zshrc
if [[ -n "$_WT_ORIG_ZDOTDIR" ]] && [[ -f "$_WT_ORIG_ZDOTDIR/.zshrc" ]]; then
    source "$_WT_ORIG_ZDOTDIR/.zshrc"
elif [[ -f "$HOME/.zshrc" ]]; then
    source "$HOME/.zshrc"
fi
# Add wt indicator to prompt
PROMPT="(wt: {}) $PROMPT"
"#,
        wt_name
    );

    std::fs::write(temp_dir.join(".zshrc"), zshrc_content)?;
    Ok(temp_dir)
}

fn show_exit_status(wt_path: &Path) -> Result<()> {
    eprintln!("\n--- Exiting wt shell ---");

    let output = Command::new("git")
        .args(["status", "--short"])
        .current_dir(wt_path)
        .output()?;

    let status = String::from_utf8_lossy(&output.stdout);
    if status.is_empty() {
        eprintln!("Working tree clean.");
    } else {
        eprintln!("Uncommitted changes:");
        eprint!("{}", status);
    }

    Ok(())
}
