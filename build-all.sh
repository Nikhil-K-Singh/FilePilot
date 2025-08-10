#!/bin/bash
# build-all.sh

echo "Building FilePilot for all platforms..."

# Create output directory
mkdir -p dist

# macOS Intel
echo "Building for macOS Intel..."
cargo build --release --target x86_64-apple-darwin
cp target/x86_64-apple-darwin/release/filepilot dist/filepilot-macos-intel

# macOS Apple Silicon
echo "Building for macOS Apple Silicon..."
cargo build --release --target aarch64-apple-darwin
cp target/aarch64-apple-darwin/release/filepilot dist/filepilot-macos-arm64

# macOS Universal Binary
echo "Building Universal Binary for macOS..."
lipo -create -output target/filepilot-universal \
    target/x86_64-apple-darwin/release/filepilot \
    target/aarch64-apple-darwin/release/filepilot
cp target/filepilot-universal dist/filepilot-macos-universal

# Windows 64-bit
# echo "Building for Windows..."
# brew install mingw-w64
# cargo build --release --target x86_64-pc-windows-gnu
# cp target/x86_64-pc-windows-gnu/release/filepilot.exe dist/filepilot-windows.exe

# Linux 64-bit
# echo "Building for Linux..."
# cross build --release --target x86_64-unknown-linux-gnu
# cp target/x86_64-unknown-linux-gnu/release/filepilot dist/filepilot-linux

# # Linux ARM64
# echo "Building for Linux ARM64..."
# cross build --release --target aarch64-unknown-linux-gnu
# cp target/aarch64-unknown-linux-gnu/release/filepilot dist/filepilot-linux-arm64

echo "All builds complete! Check the 'dist' directory."
