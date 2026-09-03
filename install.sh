#!/bin/bash
# UAPS CLI Installer for Linux/macOS
# Usage: curl -fsSL https://raw.githubusercontent.com/iyulab/U-APS/main/install.sh | bash

set -e

REPO="iyulab/U-APS"
INSTALL_DIR="$HOME/.local/bin"

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
    linux)
        case "$ARCH" in
            x86_64) RID="linux-x64" ;;
            aarch64) RID="linux-arm64" ;;
            *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        ;;
    darwin)
        case "$ARCH" in
            x86_64) RID="osx-x64" ;;
            arm64) RID="osx-arm64" ;;
            *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        ;;
    *)
        echo "Unsupported OS: $OS"
        exit 1
        ;;
esac

FILE_NAME="uaps-cli-$RID.tar.gz"

# Get latest release
echo "Fetching latest release..."
RELEASE_INFO=$(curl -s "https://api.github.com/repos/$REPO/releases/latest")
VERSION=$(echo "$RELEASE_INFO" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/$FILE_NAME"

# Create install directory
mkdir -p "$INSTALL_DIR"

# Download and extract
TEMP_FILE=$(mktemp)
echo "Downloading UAPS CLI $VERSION for $RID..."
curl -fsSL "$DOWNLOAD_URL" -o "$TEMP_FILE"

echo "Extracting to $INSTALL_DIR..."
tar -xzf "$TEMP_FILE" -C "$INSTALL_DIR"
rm "$TEMP_FILE"

# Make executable
chmod +x "$INSTALL_DIR/uaps"

# Check PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo ""
    echo "Add to PATH by adding this to your shell profile:"
    echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
fi

echo ""
echo "UAPS CLI $VERSION installed successfully!"
echo "Location: $INSTALL_DIR/uaps"
echo ""
echo "Run: uaps --help"
