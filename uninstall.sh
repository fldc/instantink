#!/bin/bash
set -e

BINARY_NAME="hp-instant-ink-cli"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

echo "HP Instant Ink CLI Uninstallation Script"
echo "========================================"

BINARY_PATH="$INSTALL_DIR/$BINARY_NAME"

if [ -f "$BINARY_PATH" ]; then
    if [ ! -w "$INSTALL_DIR" ]; then
        echo "Removing $BINARY_NAME from $INSTALL_DIR (requires sudo)..."
        sudo rm "$BINARY_PATH"
    else
        echo "Removing $BINARY_NAME from $INSTALL_DIR..."
        rm "$BINARY_PATH"
    fi
    echo "$(BINARY_NAME) has been uninstalled successfully!"
else
    echo "$(BINARY_NAME) is not installed at $BINARY_PATH"
    exit 1
fi
