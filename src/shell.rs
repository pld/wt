use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn spawn_wt_shell(wt_path: &Path, wt_name: &str, branch: &str) -> Result<()> {
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
        "bash" => spawn_bash(&shell_path, wt_path, wt_name, branch)?,
        "zsh" => spawn_zsh(&shell_path, wt_path, wt_name, branch)?,
        "fish" => spawn_fish(&shell_path, wt_path, wt_name, branch)?,
        _ => spawn_generic(&shell_path, wt_path, wt_name, branch)?,
    };

    show_exit_status(wt_path)?;
    Ok(())
}

fn spawn_bash(shell_path: &str, wt_path: &Path, wt_name: &str, branch: &str) -> Result<()> {
    let rcfile_content = format!(
        "[ -f ~/.bashrc ] && source ~/.bashrc; PS1=\"(wt: {}) $PS1\"",
        wt_name
    );
    let temp_rc = std::env::temp_dir().join(format!("wt-bashrc-{}", std::process::id()));
    std::fs::write(&temp_rc, rcfile_content)?;

    Command::new(shell_path)
        .arg("--rcfile")
        .arg(&temp_rc)
        .current_dir(wt_path)
        .envs(wt_env(wt_name, branch, wt_path))
        .status()?;

    let _ = std::fs::remove_file(&temp_rc);
    Ok(())
}

fn spawn_zsh(shell_path: &str, wt_path: &Path, wt_name: &str, branch: &str) -> Result<()> {
    let temp_dir = create_zsh_wrapper(wt_name)?;

    Command::new(shell_path)
        .current_dir(wt_path)
        .env("ZDOTDIR", &temp_dir)
        .env(
            "_WT_ORIG_ZDOTDIR",
            std::env::var("ZDOTDIR").unwrap_or_else(|_| std::env::var("HOME").unwrap_or_default()),
        )
        .envs(wt_env(wt_name, branch, wt_path))
        .status()?;

    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(())
}

fn spawn_fish(shell_path: &str, wt_path: &Path, wt_name: &str, branch: &str) -> Result<()> {
    Command::new(shell_path)
        .arg("--init-command")
        .arg(format!(
            "functions -c fish_prompt _wt_orig_prompt 2>/dev/null; \
             function fish_prompt; echo -n '(wt: {}) '; _wt_orig_prompt; end",
            wt_name
        ))
        .current_dir(wt_path)
        .envs(wt_env(wt_name, branch, wt_path))
        .status()?;
    Ok(())
}

fn spawn_generic(shell_path: &str, wt_path: &Path, wt_name: &str, branch: &str) -> Result<()> {
    Command::new(shell_path)
        .current_dir(wt_path)
        .envs(wt_env(wt_name, branch, wt_path))
        .status()?;
    Ok(())
}

fn wt_env(wt_name: &str, branch: &str, wt_path: &Path) -> Vec<(&'static str, String)> {
    vec![
        ("WT_NAME", wt_name.to_string()),
        ("WT_BRANCH", branch.to_string()),
        ("WT_PATH", wt_path.display().to_string()),
        ("WT_ACTIVE", "1".to_string()),
    ]
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
        .output()
        .context("Failed to get git status")?;

    let status = String::from_utf8_lossy(&output.stdout);
    if status.is_empty() {
        eprintln!("Working tree clean.");
    } else {
        eprintln!("Uncommitted changes:");
        eprint!("{}", status);
    }

    Ok(())
}
