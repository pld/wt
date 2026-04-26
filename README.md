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

**Session mode** manages your agents in tmux, either as panes in one shared session or as separate per-worktree sessions.

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
(wt) $              # You're now in the workspace
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
(wt) $          # Your changes are here
```

### Switch workspaces

```bash
(wt) $ wt ls
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
(wt) $
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
(wt) $ exit

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
wt session [--mode M]     Enter tmux session(s) (see Session Mode)
wt session [--mode M] ls  List workspaces in session
wt session [--mode M] add <name>
      [-b base]           base: defaults to main
      [--panes 2|3]       override pane count (panes mode) / window count (windows mode)
      [--watch]           add status window with live agent status (panes mode only)
wt session [--mode M] rm <name>
wt session [--mode M] watch [-i N]
wt -d <dir> <cmd>         Custom worktree directory (default: .worktrees)

wt new [<name>]                     Create workspace and enter it, name defaults to current branch
     [-b <base>]                    Defaults to main
     [--print-path]                 Output path only (for scripts)
wt use <name>                       Enter existing workspace
wt ls                               Interactive workspace picker
wt rm <name>                        Remove workspace (interactive if no name)
wt which                            Print current workspace name
wt session [--mode M]               Enter tmux session(s) (see Session Mode)
wt session [--mode M] ls            List workspaces in session
wt session [--mode M] add <name>    Add a named session
     [-b <base>]                    Defaults to main
     [--panes 2|3]                  Override pane count (panes mode) / window count (windows mode)
     [--watch]                      Add status window with live agent status (panes mode only)
wt session [--mode M] rm <name>     Remove a named session
wt session [--mode M] watch [-i N]  Watch all the sessions
wt -d <dir> <cmd>                   Custom worktree directory (default: .worktrees)

M = panes | windows
```

## Session Mode

Manage multiple workspaces in tmux with dedicated agent, terminal, and optional editor surfaces.

```bash
# Add workspaces to session
$ wt session add feature/auth
$ wt session add feature/payments

# List workspaces with agent status
$ wt session ls
* [0] feature/auth (active) [2 panes]    # panes mode

# Or switch the whole invocation to windows mode
$ wt session --mode windows add feature/review
$ wt session --mode windows ls
  wt-feature-review (agent: idle)

# Enter the default panes session
$ wt session

# Remove a panes-mode workspace
$ wt session rm feature/auth

# Enter or remove a windows-mode worktree session
$ wt session --mode windows
$ wt session --mode windows rm feature/review
```

`--mode` only affects the command it is passed to. Set `mode = "windows"`
in config if you want windows mode to be the default for bare
`wt session` commands.

### Layout Modes

`wt` supports two tmux layouts.

#### Panes mode (default)

All worktrees live in one shared tmux session named `wt`, one window per
worktree, split into 2 or 3 panes.

**2 panes:**
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

Use `--watch` to add a status window showing all workspaces and their agent status:

```bash
wt session add feature/auth --watch
```

- `●` green = agent active
- `○` gray = agent idle

Or run `wt session watch` manually in any pane.

#### Windows mode

Each worktree gets its own tmux session with one window per role. This is useful
on narrow screens or when you prefer window navigation over pane navigation.

- 2 windows: `agent`, `shell`
- 3 windows: `agent`, `shell`, `edit`

Session names default to `wt-<worktree>` and are configurable with
`session_prefix`.

Discovery in windows mode is state-backed: `wt` records sessions created via
`wt session add` in `~/.wt/sessions.json`, and `wt session`, `wt session ls`, and
`wt session rm` operate from that stored state. Stale entries are pruned when the
corresponding tmux session no longer exists.

Because discovery is state-backed, `session_prefix = ""` only changes naming. It
does not cause `wt` to pick up unrelated tmux sessions.

`wt session watch` and `--watch` are currently panes-mode only.

### Configuration

Create `~/.wt/config.toml` for global settings or `.wt.toml` in repo root for per-repo settings:

```toml
[session]
mode = "panes"         # "panes" (default) or "windows"
panes = 2              # 2 or 3; also used as window count in windows mode
session_prefix = "wt-" # prepended to windows-mode session names
agent_cmd = "claude"   # command for agent pane/window
editor_cmd = "nvim"    # command for editor pane/window (when panes=3)
```

Precedence: `--mode` / `--panes` flags > `.wt.toml` > `~/.wt/config.toml` > defaults

### Navigation

Standard tmux keybindings:
- `C-b` + arrow keys — switch panes
- `C-b n` / `C-b p` — next/previous window in the current tmux session
- `C-b d` — detach from session

In windows mode, `C-b n` / `C-b p` only moves between the `agent`, `shell`,
and `edit` windows for one worktree. To switch to a different worktree
session, run `wt session --mode windows` again; from inside tmux, `wt` uses
`switch-client` instead of nesting tmux sessions.

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
