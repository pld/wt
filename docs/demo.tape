# wt demo
# Run with: vhs demo.tape
# Requires: cargo build --release first, then run from a test git repo

Set Shell "bash"
Set FontSize 14
Set Width 900
Set Height 500
Set Theme "Catppuccin Mocha"
Set Padding 20
Env PATH "~/Documents/Projects/wt/target/release:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin"
Output docs/demo.gif

# Setup - we're in a repo
Type "# You are in a repo, want to run multiple AI agents in parallel"
Enter

Type "# Create isolated workspaces with wt"
Enter
Sleep 500ms

# Create first worktree
Type "wt new feature/auth --print-path"
Enter

# Create second worktree
Type "wt new feature/payments --print-path"
Enter
Sleep 100ms

# Show what we have
Type "# Each agent gets its own directory + branch"
Enter
Sleep 500ms

Type "ls .worktrees/"
Enter
Sleep 500ms

# Show the picker
Type "# Switch between them with wt ls"
Enter

Type "wt ls"
Enter
Sleep 500ms

# Arrow down and enter to show interaction
Down
Sleep 500ms
Enter
Sleep 1s

Type "# Now in the workspace"
Enter
Type "# Agent can work here safely"
Enter

Type "pwd"
Enter
Sleep 500ms

Type "git branch --show-current"
Enter
Sleep 500ms

# Exit
Type "exit"
Enter

Type "# Back in main repo. Workspaces stay until you rm."
Enter

Type "wt rm"
Enter
Sleep 500ms

# Arrow down and enter to show removal
Down
Sleep 500ms
Enter
Sleep 500ms

Type "wt ls"
Enter
Sleep 3s

