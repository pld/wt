#!/bin/bash
set -e

# Determine install directory
SYSTEM_INSTALL=false
FROM_RELEASE=false
for arg in "$@"; do
    case "$arg" in
        --system) SYSTEM_INSTALL=true ;;
        --from-release) FROM_RELEASE=true ;;
    esac
done

if [ "$SYSTEM_INSTALL" = "true" ]; then
    INSTALL_DIR="/usr/local/bin"
else
    INSTALL_DIR="$HOME/.local/bin"
fi

BIN_PATH="$INSTALL_DIR/wt"

echo "Installing wt to $BIN_PATH..."

do_install() {
    local src="$1"
    if [ "$SYSTEM_INSTALL" = "true" ] && [ "$(id -u)" -ne 0 ]; then
        sudo mkdir -p "$INSTALL_DIR"
        sudo cp "$src" "$BIN_PATH"
        sudo chmod +x "$BIN_PATH"
    else
        mkdir -p "$INSTALL_DIR"
        cp "$src" "$BIN_PATH"
        chmod +x "$BIN_PATH"
    fi
}

if [ "$FROM_RELEASE" = "true" ]; then
    echo "Downloading latest release from GitHub..."

    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)

    if [ "$ARCH" = "x86_64" ]; then
        ARCH="x86_64"
    elif [ "$ARCH" = "arm64" ] || [ "$ARCH" = "aarch64" ]; then
        ARCH="aarch64"
    fi

    if [ "$OS" = "darwin" ]; then
        PLATFORM="apple-darwin"
    elif [ "$OS" = "linux" ]; then
        PLATFORM="unknown-linux-gnu"
    else
        echo "Unsupported platform: $OS"
        exit 1
    fi

    BINARY_NAME="wt-${ARCH}-${PLATFORM}"
    DOWNLOAD_URL="https://github.com/pld/wt/releases/latest/download/${BINARY_NAME}"

    echo "Downloading from: $DOWNLOAD_URL"
    TMP_BIN=$(mktemp)
    trap 'rm -f "$TMP_BIN"' EXIT
    curl -L "$DOWNLOAD_URL" -o "$TMP_BIN"
    chmod +x "$TMP_BIN"
    do_install "$TMP_BIN"
    rm -f "$TMP_BIN"
    trap - EXIT
else
    echo "Building from source..."
    cargo build --release
    do_install "target/release/wt"
fi

# Install CLI agent skills (only if user has the tool configured)
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GITHUB_RAW="https://raw.githubusercontent.com/pld/wt/main/commands"

install_claude_skill() {
    if [ -d "$HOME/.claude" ]; then
        CLAUDE_COMMANDS_DIR="$HOME/.claude/commands"
        echo "Installing Claude Code skills..."
        mkdir -p "$CLAUDE_COMMANDS_DIR"
        if [ -d "$SCRIPT_DIR/commands" ]; then
            cp "$SCRIPT_DIR/commands/"*.md "$CLAUDE_COMMANDS_DIR/" 2>/dev/null || true
        else
            curl -fsSL "$GITHUB_RAW/do.md" -o "$CLAUDE_COMMANDS_DIR/do.md" 2>/dev/null || true
        fi
    fi
}

install_gemini_skill() {
    if [ -d "$HOME/.gemini" ]; then
        GEMINI_COMMANDS_DIR="$HOME/.gemini/commands"
        echo "Installing Gemini CLI commands..."
        mkdir -p "$GEMINI_COMMANDS_DIR"
        if [ -d "$SCRIPT_DIR/commands" ]; then
            cp "$SCRIPT_DIR/commands/"*.toml "$GEMINI_COMMANDS_DIR/" 2>/dev/null || true
        else
            curl -fsSL "$GITHUB_RAW/do.toml" -o "$GEMINI_COMMANDS_DIR/do.toml" 2>/dev/null || true
        fi
    fi
}

install_claude_skill
install_gemini_skill

# Migrate legacy ~/.wt/ layout to XDG locations.
# Mirror the Rust helpers: only accept absolute XDG paths; fall back to defaults
# for empty or relative values so migration never writes to cwd-relative locations.
xdg_abs() {
    local val="$1" default="$2"
    case "$val" in
        /*) echo "$val" ;;
        *)  echo "$default" ;;
    esac
}
XDG_CONFIG_HOME=$(xdg_abs "${XDG_CONFIG_HOME:-}" "$HOME/.config")
XDG_STATE_HOME=$(xdg_abs "${XDG_STATE_HOME:-}" "$HOME/.local/state")

# Remove the exact comment+alias pair the old installer wrote from a shell rc file.
# Only the two-line block is removed:
#   # wt - Git worktree orchestrator
#   alias wt=...  (or: alias wt '...' for fish)
# Standalone alias lines the user may have written themselves are left alone.
# awk is used instead of sed -i because BSD sed (macOS) requires -i ''
# while GNU sed requires -i, with no portable common form.
remove_wt_alias() {
    local rc="$1"
    local tmp
    tmp=$(mktemp) || return 1
    awk '
        /^# wt - Git worktree orchestrator$/ {
            if ((getline next_line) > 0 && next_line ~ /^alias wt[= '"'"']/) {
                next
            }
            print
            print next_line
            next
        }
        { print }
    ' "$rc" > "$tmp" && mv "$tmp" "$rc" || rm -f "$tmp"
}

migrate_legacy() {
    for rc in "$HOME/.bashrc" "$HOME/.bash_profile" "$HOME/.zshrc" "$HOME/.config/fish/config.fish"; do
        if [ ! -f "$rc" ]; then
            continue
        fi
        if grep -q "^# wt - Git worktree orchestrator$" "$rc" 2>/dev/null; then
            echo "Removing legacy wt alias from $rc..."
            remove_wt_alias "$rc"
        fi
    done

    # Remove the legacy binary now that the new one is in place.
    if [ -f "$HOME/.wt/wt" ]; then
        echo "Removing legacy binary ~/.wt/wt..."
        rm "$HOME/.wt/wt"
    fi

    # Migrate global config if only the legacy location exists.
    LEGACY_CONFIG="$HOME/.wt/config.toml"
    NEW_CONFIG="$XDG_CONFIG_HOME/wt/config.toml"
    if [ -f "$LEGACY_CONFIG" ] && [ ! -f "$NEW_CONFIG" ]; then
        echo "Migrating $LEGACY_CONFIG -> $NEW_CONFIG..."
        mkdir -p "$(dirname "$NEW_CONFIG")"
        mv "$LEGACY_CONFIG" "$NEW_CONFIG"
    fi

    # Migrate session state if only the legacy location exists.
    LEGACY_STATE="$HOME/.wt/sessions.json"
    NEW_STATE="$XDG_STATE_HOME/wt/sessions.json"
    if [ -f "$LEGACY_STATE" ] && [ ! -f "$NEW_STATE" ]; then
        echo "Migrating $LEGACY_STATE -> $NEW_STATE..."
        mkdir -p "$(dirname "$NEW_STATE")"
        mv "$LEGACY_STATE" "$NEW_STATE"
    fi

    # Remove ~/.wt/ if it is now empty.
    if [ -d "$HOME/.wt" ] && [ -z "$(ls -A "$HOME/.wt")" ]; then
        echo "Removing empty ~/.wt/ directory..."
        rmdir "$HOME/.wt"
    fi
}

migrate_legacy

echo ""
echo "Installed wt to $BIN_PATH"

# Warn if the install directory is not on PATH.
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
    echo ""
    echo "Note: $INSTALL_DIR is not on your PATH."
    echo "Add this line to your shell config to make 'wt' available:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
fi
