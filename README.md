# wt

Parallel workspaces for agent sandboxes.

## What It Does

`wt` creates isolated git worktrees where AI agents can work without stepping on each other. Each workspace is its own branch—agents work in parallel, you merge results when done.

- **Run multiple agents simultaneously** — each in their own workspace, no conflicts
- **Instant context switching** — jump between tasks without stashing or committing
- **Branch isolation** — agents can't accidentally modify each other's work
- **Zero overhead** — workspaces share git history, no disk duplication
- **Simple cleanup** — remove workspaces when done, branches stay for merging

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
wt -d <dir> <cmd>         Custom worktree directory (default: .worktrees)
```

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

MIT
