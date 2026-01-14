#!/bin/bash
set -e

INSTALL_DIR="$HOME/.wt"
BIN_PATH="$INSTALL_DIR/wt"

echo "Installing wt..."

mkdir -p "$INSTALL_DIR"

if [ -f "$BIN_PATH" ]; then
    echo "Removing existing installation..."
    rm "$BIN_PATH"
fi

if [ "$1" = "--from-release" ]; then
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
    curl -L "$DOWNLOAD_URL" -o "$BIN_PATH"
    chmod +x "$BIN_PATH"
else
    echo "Building from source..."
    cargo build --release
    cp target/release/wt "$BIN_PATH"
fi

CURRENT_SHELL=$(basename "$SHELL")

setup_shell_config() {
    case "$CURRENT_SHELL" in
        bash)
            if [ -f "$HOME/.bash_profile" ] && [ "$(uname)" = "Darwin" ]; then
                echo "$HOME/.bash_profile"
            else
                echo "$HOME/.bashrc"
            fi
            ;;
        zsh)
            echo "$HOME/.zshrc"
            ;;
        fish)
            echo "$HOME/.config/fish/config.fish"
            ;;
        *)
            echo ""
            ;;
    esac
}

SHELL_CONFIG=$(setup_shell_config)
ALIAS_LINE="alias wt='$BIN_PATH'"
FISH_ALIAS="alias wt '$BIN_PATH'"

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

if [ -n "$SHELL_CONFIG" ]; then
    mkdir -p "$(dirname "$SHELL_CONFIG")"
    if [ "$CURRENT_SHELL" = "fish" ]; then
        if ! grep -q "alias wt " "$SHELL_CONFIG" 2>/dev/null; then
            echo "" >> "$SHELL_CONFIG"
            echo "# wt - Git worktree orchestrator" >> "$SHELL_CONFIG"
            echo "$FISH_ALIAS" >> "$SHELL_CONFIG"
            echo "Added alias to $SHELL_CONFIG"
        else
            echo "Alias already exists in $SHELL_CONFIG"
        fi
    else
        if ! grep -q "alias wt=" "$SHELL_CONFIG" 2>/dev/null; then
            echo "" >> "$SHELL_CONFIG"
            echo "# wt - Git worktree orchestrator" >> "$SHELL_CONFIG"
            echo "$ALIAS_LINE" >> "$SHELL_CONFIG"
            echo "Added alias to $SHELL_CONFIG"
        else
            echo "Alias already exists in $SHELL_CONFIG"
        fi
    fi
    echo ""
    echo "Installed to $BIN_PATH"
else
    echo ""
    echo "Installed to $BIN_PATH"
    echo ""
    echo "Add this to your shell config:"
    echo "  $ALIAS_LINE"
fi

exec $SHELL
