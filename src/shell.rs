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
        _ => spawn_shell(shell_cmd(&shell_path, wt_path, wt_name, branch))?,
    };

    show_exit_status(wt_path)?;
    Ok(())
}

fn shell_cmd(shell_path: &str, wt_path: &Path, wt_name: &str, branch: &str) -> Command {
    let mut cmd = Command::new(shell_path);
    cmd.current_dir(wt_path)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env("WT_NAME", wt_name)
        .env("WT_BRANCH", branch)
        .env("WT_PATH", wt_path.display().to_string())
        .env("WT_ACTIVE", "1");
    cmd
}

fn spawn_shell(mut cmd: Command) -> Result<()> {
    cmd.status()?;
    Ok(())
}

fn spawn_bash(shell_path: &str, wt_path: &Path, wt_name: &str, branch: &str) -> Result<()> {
    let rcfile_content = "[ -f ~/.bashrc ] && source ~/.bashrc; PS1=\"(wt) $PS1\"".to_string();
    let temp_rc = std::env::temp_dir().join(format!("wt-bashrc-{}", std::process::id()));
    std::fs::write(&temp_rc, &rcfile_content)?;

    let mut cmd = shell_cmd(shell_path, wt_path, wt_name, branch);
    cmd.arg("--rcfile").arg(&temp_rc);
    spawn_shell(cmd)?;

    let _ = std::fs::remove_file(&temp_rc);
    Ok(())
}

fn spawn_zsh(shell_path: &str, wt_path: &Path, wt_name: &str, branch: &str) -> Result<()> {
    let temp_dir = create_zsh_wrapper()?;

    let mut cmd = shell_cmd(shell_path, wt_path, wt_name, branch);
    cmd.env("ZDOTDIR", &temp_dir).env(
        "_WT_ORIG_ZDOTDIR",
        std::env::var("ZDOTDIR").unwrap_or_else(|_| std::env::var("HOME").unwrap_or_default()),
    );
    spawn_shell(cmd)?;

    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(())
}

fn spawn_fish(shell_path: &str, wt_path: &Path, wt_name: &str, branch: &str) -> Result<()> {
    let mut cmd = shell_cmd(shell_path, wt_path, wt_name, branch);
    cmd.arg("--init-command").arg(
        "functions -c fish_prompt _wt_orig_prompt 2>/dev/null; \
             function fish_prompt; echo -n '(wt) '; _wt_orig_prompt; end",
    );
    spawn_shell(cmd)
}

fn create_zsh_wrapper() -> Result<PathBuf> {
    let temp_dir = std::env::temp_dir().join(format!("wt-zsh-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir)?;
    let functions_dir = temp_dir.join("functions");
    std::fs::create_dir_all(&functions_dir)?;

    // zsh reads `.zshenv` before `.zshrc`, so this is the earliest safe place
    // to restore the real dotdir and install the completion shim.
    let zshenv_content = r#"# Pre-compinit compdef stub to prevent "command not found" errors
# Make the temp functions directory visible before the real startup files load.
fpath=("$ZDOTDIR/functions" $fpath)

# Restore the original dotdir before zsh loads the rest of the startup chain.
if [[ -n "$_WT_ORIG_ZDOTDIR" ]]; then
    export ZDOTDIR="$_WT_ORIG_ZDOTDIR"
fi

# Hook compinit to replay queued compdef calls.
function _wt_replay_compdef {
    unfunction compdef 2>/dev/null
    autoload -Uz compdef
    typeset -a _wt_compdef_queue
    for cmd in "${_wt_compdef_queue[@]}"; do
        eval "compdef $cmd"
    done
    unset _wt_compdef_queue
    unfunction _wt_replay_compdef
    _wt_install_prompt_prefix
}

function _wt_apply_prompt_prefix {
    [[ $PROMPT == \(wt\)* ]] || PROMPT="(wt) $PROMPT"
}

function _wt_install_prompt_prefix {
    typeset -ga precmd_functions
    precmd_functions=(${precmd_functions:#_wt_apply_prompt_prefix})
    # `precmd` runs after framework prompt setup, so this preserves the theme.
    precmd_functions+=(_wt_apply_prompt_prefix)
}

# Wrap compinit to replay after it runs.
function compinit {
    unfunction compinit
    autoload -Uz compinit
    compinit "$@"
    _wt_replay_compdef
}

_wt_install_prompt_prefix
"#;

    let compdef_content = r#"# Pre-compinit compdef stub to prevent "command not found" errors.
typeset -ga _wt_compdef_queue

compdef() {
    _wt_compdef_queue+=("${(j: :)${(q)@}}")
}
"#;

    std::fs::write(temp_dir.join(".zshenv"), zshenv_content)?;
    std::fs::write(functions_dir.join("compdef"), compdef_content)?;
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

#[cfg(test)]
mod tests {
    use super::create_zsh_wrapper;
    use std::fs;
    use std::process::Command;

    fn zsh_available() -> bool {
        Command::new("zsh")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn zsh_wrapper_sources_startup_files_from_original_dotdir() {
        let temp_dir = create_zsh_wrapper().expect("create zsh wrapper");
        let zshenv = std::fs::read_to_string(temp_dir.join(".zshenv")).expect("read zshenv");

        assert!(zshenv.contains("export ZDOTDIR=\"$_WT_ORIG_ZDOTDIR\""));
        assert!(zshenv.contains("fpath=(\"$ZDOTDIR/functions\" $fpath)"));
        assert!(temp_dir.join("functions").join("compdef").exists());
        assert!(!temp_dir.join(".zshrc").exists());

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn zsh_wrapper_boots_framework_like_startup_without_compdef_errors() {
        if !zsh_available() {
            return;
        }

        let home_dir = tempfile::TempDir::new().expect("create fake home");
        fs::write(
            home_dir.path().join(".zshrc"),
            r#"autoload -Uz compinit
compdef _git git
compinit
PROMPT="demo ❯❯❯ "
"#,
        )
        .expect("write fake zshrc");

        let wrapper_dir = create_zsh_wrapper().expect("create zsh wrapper");

        let output = Command::new("zsh")
            .arg("-ic")
            .arg("print -r -- \"${(j:,:)precmd_functions}\"; _wt_apply_prompt_prefix; print -r -- \"$PROMPT\"")
            .env("HOME", home_dir.path())
            .env("ZDOTDIR", &wrapper_dir)
            .env("_WT_ORIG_ZDOTDIR", home_dir.path())
            .output()
            .expect("run zsh startup");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "zsh startup failed: {}", stderr);
        assert!(
            !stderr.contains("compdef:"),
            "unexpected compdef error output: {}",
            stderr
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("_wt_apply_prompt_prefix"));
        assert!(stdout.contains("(wt) demo ❯❯❯ "));

        let _ = fs::remove_dir_all(wrapper_dir);
    }
}
