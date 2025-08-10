#!/bin/bash

# FilePilot Installation Script
# This script builds FilePilot and sets up configuration in the user's home directory

set -e

echo "Building FilePilot..."
cargo build --release

echo "Setting up configuration..."

# Create .filepilot directory in user's home
CONFIG_DIR="$HOME/.filepilot"
mkdir -p "$CONFIG_DIR"

# Copy default config if it doesn't exist
if [ ! -f "$CONFIG_DIR/config.json" ]; then
    if [ -f "src/config.json" ]; then
        cp "src/config.json" "$CONFIG_DIR/config.json"
        echo "✅ Default configuration copied to $CONFIG_DIR/config.json"
    else
        echo "⚠️  No default config found. You can create one with: ./target/release/filepilot --create-config"
    fi
else
    echo "ℹ️  Configuration already exists at $CONFIG_DIR/config.json"
fi

# Make the binary easily accessible
BINARY_PATH="./target/release/filepilot"
echo "🎉 Build complete!"
echo ""
echo "FilePilot is ready to use:"
echo "  • Binary location: $BINARY_PATH"
echo "  • Configuration: $CONFIG_DIR/config.json"
echo ""
echo "To use FilePilot globally, you can:"
echo "  1. Add the binary to your PATH, or"
echo "  2. Create a symlink: ln -s $(pwd)/target/release/filepilot /usr/local/bin/filepilot"
echo ""
echo "To run FilePilot:"
echo "  $BINARY_PATH"
echo ""
echo "To customize key bindings:"
echo "  Edit $CONFIG_DIR/config.json"
echo ""
echo "To create a fresh config file:"
echo "  $BINARY_PATH --create-config"
