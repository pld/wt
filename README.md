# wt

Parallel workspaces for agent sandboxes.

## What It Does

`wt` creates isolated git worktrees where AI agents can work without stepping on each other. Each workspace is its own branch—agents work in parallel, you merge results when done.

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/pld/wt/main/install.sh | bash -s -- --from-release
source ~/.zshrc
```

Or build from source:
```bash
git clone https://github.com/pld/wt.git && cd wt && ./install.sh
```

## Usage

### Interactive Mode

```bash
wt new feat-auth          # Create workspace (new branch from main)
wt new feat-pay -b develop  # Create from different base branch
wt ls                      # List workspaces
wt rm feat-auth            # Remove workspace
```

Then open terminals in each workspace:
```bash
cd ../wt-trees/feat-auth   # Terminal 1: agent works here
cd ../wt-trees/feat-pay    # Terminal 2: another agent here
```

Merge when done:
```bash
git merge feat-auth        # From main branch
```

### Batch Mode

For automated orchestration, use a config file:

```yaml
# tasks.yaml
base_branch: main

tasks:
  - id: feat-auth
    prompt: "Implement OAuth2"
    agent: claude-code

  - id: feat-payments
    prompt: "Add Stripe"
    agent: claude-code

merge_strategy: squash
cleanup: auto
```

```bash
wt run tasks.yaml          # Run all tasks in parallel
wt run tasks.yaml --dry-run  # Preview what would happen
```

## CLI Reference

```
wt new <name> [-b base]   Create workspace (default base: main)
wt ls                     List all workspaces
wt rm <name>              Remove workspace
wt run <config>           Batch orchestration from YAML
wt -d <dir> <cmd>         Use custom worktree directory (default: ../wt-trees)
```

## How It Works

Each workspace is a git worktree—a separate directory with its own branch, sharing the same `.git`:

```
~/myrepo/                  # main branch (your primary checkout)
~/wt-trees/feat-auth/      # feat-auth branch (workspace 1)
~/wt-trees/feat-pay/       # feat-pay branch (workspace 2)
```

No disk duplication. All branches share history. Standard git merge/rebase works.

## Config Reference (Batch Mode)

```yaml
base_branch: main              # Required: branch to fork from
worktree_dir: ../wt-trees      # Optional: where to create workspaces

tasks:
  - id: feat-auth              # Branch/workspace name
    prompt: "Description"      # What the task does
    agent: claude-code         # Command to run

merge_strategy: squash         # squash | rebase | manual
cleanup: auto                  # auto | manual | keep-on-error
```

## Development

```bash
cargo build
cargo test
cargo build --release    # 1.5MB binary
```

## Releasing

Push to `main` → GitHub Actions builds all platforms → `latest` release updated.

Platforms: Linux x86_64/ARM64, macOS Intel/Apple Silicon, Windows x86_64

## License

MIT
