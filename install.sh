#!/usr/bin/env bash
# DeskPilot macOS & Linux 1-Click Installer
# Usage: curl -fsSL https://raw.githubusercontent.com/thomsonworks-in/deskpilot/main/install.sh | sh

set -eu

REPO="thomsonworks-in/deskpilot"
BINARY_NAME="deskpilot"
INSTALL_DIR="$HOME/.deskpilot/bin"

echo "========================================="
echo "     DeskPilot macOS/Linux Installer      "
echo "========================================="

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux*)
        case "$ARCH" in
            x86_64) TARGET="x86_64-unknown-linux-gnu" ;;
            aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
            *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        ;;
    Darwin*)
        case "$ARCH" in
            x86_64) TARGET="x86_64-apple-darwin" ;;
            arm64) TARGET="aarch64-apple-darwin" ;;
            *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        ;;
    *)
        echo "Unsupported OS: $OS"; exit 1 ;;
esac

echo "Detected Target: $TARGET"
TAG=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' || echo "v0.1.0")
if [ -z "$TAG" ]; then TAG="v0.1.0"; fi

ARCHIVE="deskpilot-$TARGET.tar.gz"
DOWNLOAD_URL="https://github.com/$REPO/releases/download/$TAG/$ARCHIVE"

mkdir -p "$INSTALL_DIR"
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

echo "Downloading DeskPilot ($ARCHIVE)..."
curl -fSL "$DOWNLOAD_URL" -o "$TMP_DIR/$ARCHIVE"

echo "Extracting..."
tar -xzf "$TMP_DIR/$ARCHIVE" -C "$INSTALL_DIR"
chmod +x "$INSTALL_DIR/$BINARY_NAME"

SHELL_RC=""
case "$SHELL" in
    */zsh) SHELL_RC="$HOME/.zshrc" ;;
    */bash) SHELL_RC="$HOME/.bashrc" ;;
    *) SHELL_RC="$HOME/.profile" ;;
esac

if ! grep -q "$INSTALL_DIR" "$SHELL_RC" 2>/dev/null; then
    echo "export PATH="\$PATH:$INSTALL_DIR"" >> "$SHELL_RC"
    echo "Added DeskPilot to $SHELL_RC"
fi

echo ""
echo "DeskPilot successfully installed to $INSTALL_DIR/$BINARY_NAME"
echo "Run 'source $SHELL_RC' or restart your terminal, then type 'deskpilot'!"
echo "========================================="
