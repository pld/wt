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

SHELL_CONFIG=""
if [ -n "$BASH_VERSION" ]; then
    SHELL_CONFIG="$HOME/.bashrc"
elif [ -n "$ZSH_VERSION" ]; then
    SHELL_CONFIG="$HOME/.zshrc"
else
    CURRENT_SHELL=$(basename "$SHELL")
    if [ "$CURRENT_SHELL" = "bash" ]; then
        SHELL_CONFIG="$HOME/.bashrc"
    elif [ "$CURRENT_SHELL" = "zsh" ]; then
        SHELL_CONFIG="$HOME/.zshrc"
    fi
fi

ALIAS_LINE="alias wt='$BIN_PATH'"

if [ -n "$SHELL_CONFIG" ]; then
    if ! grep -q "alias wt=" "$SHELL_CONFIG" 2>/dev/null; then
        echo "" >> "$SHELL_CONFIG"
        echo "# wt - Git worktree orchestrator" >> "$SHELL_CONFIG"
        echo "$ALIAS_LINE" >> "$SHELL_CONFIG"
        echo "Added alias to $SHELL_CONFIG"
    else
        echo "Alias already exists in $SHELL_CONFIG"
    fi
else
    echo "Could not detect shell config. Add this manually to your shell config:"
    echo "  $ALIAS_LINE"
fi

echo ""
echo "✓ wt installed to $BIN_PATH"
echo ""
echo "Reload your shell or run:"
echo "  source $SHELL_CONFIG"
echo ""
echo "Then try:"
echo "  wt --help"
echo "  wt example-tasks.yaml --dry-run"
