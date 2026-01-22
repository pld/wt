# wt

[![Tests](https://github.com/pld/wt/actions/workflows/test.yml/badge.svg)](https://github.com/pld/wt/actions/workflows/test.yml)

Run multiple AI agents on the same codebase without them clobbering each other.

## The Problem

You're using Claude Code, Cursor, Aider, or similar. You want to run 3 agents on 3 different tasks. But they all want to edit the same files, and now you're playing traffic cop.

Git worktrees solve this—but the commands are verbose, cleanup is manual, and you're forgetting which worktree is where.

## The Solution

`wt` creates isolated git worktrees—each agent gets its own directory, its own branch, full access to the repo. They work in parallel, you merge when done.

![wt demo](docs/demo.gif)

```
~/myrepo/                     # main - you're here
~/myrepo/.worktrees/auth/     # agent 1 working on auth
~/myrepo/.worktrees/payments/ # agent 2 working on payments
~/myrepo/.worktrees/bugfix/   # agent 3 fixing that bug
```

**Session mode** manages all your agents in one tmux session with a live status dashboard showing which agents are active or idle.

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/pld/wt/main/install.sh | bash -s -- --from-release && exec $SHELL
```

Or build from source:
```bash
git clone https://github.com/pld/wt.git && cd wt && ./install.sh
```

## Usage

### Create a workspace

```bash
$ wt new feature/auth
Entering worktree: feature/auth
(wt: feature/auth) $              # You're now in the workspace
```

Or from a different base branch:
```bash
$ wt new hotfix-login -b develop
```

If you're already on a feature branch, just run `wt new` to move your work to a workspace:
```bash
$ git checkout -b feature/payments
# ... make some changes ...
$ wt new
Stashing uncommitted changes...
Switching to main...
Entering worktree: feature/payments
(wt: feature/payments) $          # Your changes are here
```

### Switch workspaces

```bash
(wt: feature/auth) $ wt ls
Select worktree:
> feature/auth *
  feature/payments
  bugfix/header
  ← cancel
```

Use arrow keys to select, Enter to switch.

### Enter existing workspace

```bash
$ wt use feature/payments
Entering worktree: feature/payments
(wt: feature/payments) $
```

### Remove a workspace

```bash
$ wt rm
Remove worktree:
> feature/auth
  feature/payments
  bugfix/header
  ← cancel
```

Or directly: `wt rm feature/auth`

### Exit workspace

```bash
(wt: feature/auth) $ exit

--- Exiting wt shell ---
Uncommitted changes:
 M src/auth.rs
$                                 # Back in main repo
```

### Merge when done

```bash
$ git merge feature/auth
```

## CLI Reference

```
wt new [name] [-b base]   Create workspace and enter it
      [--print-path]      name: defaults to current branch
                          base: defaults to main
                          --print-path: output path only (for scripts)
wt use [name]             Enter existing workspace
wt ls                     Interactive workspace picker
wt rm [name]              Remove workspace (interactive if no name)
wt which                  Print current workspace name
wt session                Attach to tmux session (see Session Mode)
wt session ls             List workspaces in session (with agent status)
wt session add <name>     Add workspace to session
      [-b base]           base: defaults to main
      [--panes 2|3]       override pane count
      [--watch]           add status window with live agent status
wt session rm <name>      Remove workspace from session
wt session watch [-i N]   Live status dashboard (or use --watch above)
wt -d <dir> <cmd>         Custom worktree directory (default: .worktrees)
```

## Session Mode

Manage multiple workspaces in a tmux session with dedicated panes for AI agents, terminal, and optionally an editor.

```bash
# Add workspaces to session
$ wt session add feature/auth
$ wt session add feature/payments

# List workspaces with agent status
$ wt session ls
* [0] feature/auth (active) [2 panes]    # agent running
  [1] feature/payments (idle) [2 panes]   # agent at shell prompt

# Attach to session (or switch if detached)
$ wt session

# Remove workspace from session
$ wt session rm feature/auth

# Commands work from inside the session too
# (switches windows instead of attaching)
```

### Status Window

Use `--watch` to add a status window showing all workspaces and their agent status:

```bash
wt session add feature/auth --watch
```

- `●` green = agent active (running a command)
- `○` gray = agent idle (at shell prompt)

Or run `wt session watch` manually in any pane.

### Pane Layouts

**2 panes (default):**
```
+---------------------------+---------------------------+
|                           |                           |
|  Agent CLI                |  Terminal                 |
|  (claude, aider, etc.)    |  (tests, git, etc.)       |
|                           |                           |
+---------------------------+---------------------------+
```

**3 panes:**
```
+---------------------------+---------------------------+
|  Agent CLI                |                           |
|  (claude, aider, etc.)    |                           |
+---------------------------+        Editor             |
|  Terminal                 |        (nvim, vim)        |
|  (tests, git, etc.)       |                           |
+---------------------------+---------------------------+
```

### Configuration

Create `~/.wt/config.toml` for global settings or `.wt.toml` in repo root for per-repo settings:

```toml
[session]
panes = 2           # 2 or 3 (default: 2)
agent_cmd = "claude"  # command for agent pane
editor_cmd = "nvim"   # command for editor pane (when panes=3)
```

Precedence: `--panes` flag > `.wt.toml` > `~/.wt/config.toml` > defaults

### Navigation

Standard tmux keybindings:
- `C-b` + arrow keys — switch panes
- `C-b n` / `C-b p` — next/previous window
- `C-b d` — detach from session

### Environment Variables

Inside a workspace shell:
- `WT_NAME` - Workspace name
- `WT_BRANCH` - Git branch
- `WT_PATH` - Full path to workspace
- `WT_ACTIVE` - Set to "1"

## How It Works

```
~/myrepo/                              # main branch
~/myrepo/.worktrees/feature--auth/     # feature/auth workspace
~/myrepo/.worktrees/feature--payments/ # feature/payments workspace
```

Each workspace is a git worktree—separate directory, own branch, shared `.git`. No disk duplication. Standard git merge/rebase works.

### Git Push

Workspaces are configured with upstream tracking automatically. Just `git push`—no need for `-u origin HEAD`.

### Local Files (.env, etc.)

Gitignored files like `.env` aren't copied to worktrees by default. To symlink them automatically, add a `# wt copy` section to your `.gitignore`:

```gitignore
node_modules/
*.log

# wt copy
.env
.env.local
config/local_settings.py
```

Files listed after `# wt copy` (until the next `#` comment or blank line) will be symlinked from the main repo into new workspaces.

## AI Agent Integration

Installation includes a `/do` command for Claude Code and Gemini CLI (installed only if you have them configured):

```
/do gh 123      # Work on GitHub issue #123 in isolated worktree
/do sc 45678    # Work on Shortcut story in isolated worktree
```

The command automatically:
1. Fetches issue/story details (uses Shortcut MCP if configured)
2. Creates an isolated worktree (uses branch name from Shortcut metadata)
3. Works on the task
4. Commits with issue reference

## License

Apache-2.0
