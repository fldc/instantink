#!/bin/bash
set -e

BINARY_NAME="hp-instant-ink-cli"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
BUILD_MODE="${BUILD_MODE:-release}"

echo "HP Instant Ink CLI Installation Script"
echo "======================================"

if ! command -v cargo &> /dev/null; then
    echo "Error: Cargo is not installed. Please install Rust and Cargo first."
    exit 1
fi

echo "Building $BINARY_NAME in $BUILD_MODE mode..."
if [ "$BUILD_MODE" = "release" ]; then
    cargo build --release
    BINARY_PATH="target/release/$BINARY_NAME"
else
    cargo build
    BINARY_PATH="target/debug/$BINARY_NAME"
fi

if [ ! -f "$BINARY_PATH" ]; then
    echo "Error: Build failed. Binary not found at $BINARY_PATH"
    exit 1
fi

echo "Binary built successfully at $BINARY_PATH"

if [ ! -w "$INSTALL_DIR" ]; then
    echo "Installing $BINARY_NAME to $INSTALL_DIR (requires sudo)..."
    sudo cp "$BINARY_PATH" "$INSTALL_DIR/$BINARY_NAME"
    sudo chmod +x "$INSTALL_DIR/$BINARY_NAME"
else
    echo "Installing $BINARY_NAME to $INSTALL_DIR..."
    cp "$BINARY_PATH" "$INSTALL_DIR/$BINARY_NAME"
    chmod +x "$INSTALL_DIR/$BINARY_NAME"
fi

echo "Installation complete!"
echo "You can now run: $BINARY_NAME --help"
echo ""
echo "Examples:"
echo "  $BINARY_NAME --printer 192.168.1.13"
echo "  $BINARY_NAME config --set-printer 192.168.1.13"
echo "  $BINARY_NAME --printer hp-printer.local --format json"
