# /do - Work on an issue in an isolated worktree

Work on a GitHub issue or Shortcut story in an isolated git worktree.

## Usage

```
/do gh <issue-number>    # GitHub issue
/do sc <story-id>        # Shortcut story
```

## Instructions

When the user invokes `/do`, follow these steps:

### 1. Parse the argument

Extract the source and ID:
- `gh <number>` → GitHub issue
- `sc <number>` → Shortcut story

### 2. Fetch issue details

**For GitHub issues:**
```bash
gh issue view <number> --json title,body,labels,assignees
```

**For Shortcut stories:**

First, try using the Shortcut MCP if available (check for `shortcut` or `shortcut-mcp` in configured MCP servers). Use the MCP tool to fetch story details.

If no MCP is configured, fall back to the API:
```bash
curl -s -H "Shortcut-Token: $SHORTCUT_API_TOKEN" \
  "https://api.app.shortcut.com/api/v3/stories/<id>"
```

### 3. Create isolated worktree

**For Shortcut stories:** Use the branch name from the story metadata (`branches[0].name` or similar field). Shortcut stories include their associated branch name.

**For GitHub issues:** Derive a branch name (e.g., `gh-123-fix-login-bug`).

```bash
wt new <branch-name> --print-path
```

This prints the worktree path. Capture it.

### 4. Set working context

All subsequent file operations should use the worktree path as the working directory. When running commands, use `cd <worktree-path> && <command>` or run them with the worktree as cwd.

### 5. Summarize and begin

Tell the user:
- What issue/story you're working on (title, key details)
- The worktree branch name
- That you're now working in isolation

Then begin working on the task described in the issue.

### 6. When done

After completing the work:
- Commit changes with a message referencing the issue (`Fixes #123` or `[sc-45678]`)
- Offer to create a PR with `gh pr create`
- Remind user they can `wt rm <branch>` to clean up after merge

## Example

User: `/do gh 42`