# 🗂️ FilePilot

A modern terminal-based file explorer with powerful file sharing capabilities. Navigate your filesystem with ease and instantly share files across your network with a beautiful web interface.

<img width="1650" height="1050" alt="Screenshot 2025-07-30 at 9 34 29 PM" src="https://github.com/user-attachments/assets/5cb6a988-e277-4f58-bbdb-a46e66202854" />

## ✨ Features

### 🖥️ Terminal File Explorer
- **Beautiful TUI** - Intuitive terminal user interface with keyboard navigation
- **Fast Navigation** - Quick directory browsing with vim-like keybindings
- **File Search** - Find files quickly across your system
- **File Operations** - Copy, move, delete files and directories

### 🌐 Instant File Sharing
- **One-Key Sharing** - Press 'S' to instantly share any file
- **Auto URL Copy** - Sharing URL automatically copied to clipboard
- **Web Viewer** - Professional dark-themed web interface for file viewing
- **Cross Platform** - Share files between any devices on your network

### 📁 Comprehensive File Support
- **🎥 Videos**: MP4, WebM, OGV, MOV, AVI, MKV, M4V, WMV, FLV (with HTTP range requests for smooth streaming)
- **🎵 Audio**: MP3, WAV, M4A, AAC, OGA, OGG, FLAC (with full browser controls)
- **🖼️ Images**: JPG, PNG, GIF, SVG, WebP, BMP, ICO (direct browser display)
- **💻 Code**: Python, Rust, JavaScript, HTML, CSS, C/C++, Java, Go, PHP, Ruby, Swift, Kotlin (with syntax highlighting)
- **📄 Documents**: PDF, TXT, Markdown, Log files (with proper formatting)
- **📊 Data**: JSON, GeoJSON, XML, YAML, TOML (with syntax highlighting and formatting)
- **📋 Spreadsheets**: CSV, XLSX, XLS (rendered as interactive tables)
- **📓 Notebooks**: Jupyter (.ipynb) files with full cell rendering

### 🎨 Advanced Viewing Features
- **Syntax Highlighting** - Beautiful code display with Prism.js
- **JSON/GeoJSON Formatting** - Server-side formatting for large files (handles 20MB+ files)
- **Markdown Rendering** - GitHub-style markdown with proper styling
- **Jupyter Notebooks** - Full notebook rendering with code cells and outputs
- **Spreadsheet Tables** - Professional table display with sorting and filtering
- **Dark Theme** - Consistent dark theme across all file viewers

## 🚀 Installation

### Prerequisites
- **Rust** (1.70 or higher)
- **Git**

### Build from Source
```bash
# Clone the repository
git clone https://github.com/Nikhil-K-Singh/FilePilot.git
cd FilePilot

# Build the project
cargo build --release

# Run FilePilot
./target/release/filepilot
```

## 📦 Cross-Platform Binaries

### Building for Multiple Platforms

#### 1. Install Cross-Compilation Targets
```bash
# For Windows (from Mac/Linux)
rustup target add x86_64-pc-windows-gnu

# For Linux (from Mac/Windows)
rustup target add x86_64-unknown-linux-gnu

# For macOS (from Linux/Windows)
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin  # For Apple Silicon

# For ARM Linux (Raspberry Pi, etc.)
rustup target add aarch64-unknown-linux-gnu
rustup target add armv7-unknown-linux-gnueabihf
```

#### 2. Build for Specific Platforms

##### 🍎 macOS
```bash
# Intel Mac
cargo build --release --target x86_64-apple-darwin

# Apple Silicon (M1/M2)
cargo build --release --target aarch64-apple-darwin

# Universal Binary (both Intel and Apple Silicon)
# First build both targets, then combine:
lipo -create -output target/filepilot-universal \
    target/x86_64-apple-darwin/release/filepilot \
    target/aarch64-apple-darwin/release/filepilot
```

##### 🪟 Windows
```bash
# 64-bit Windows
cargo build --release --target x86_64-pc-windows-gnu

# Note: You might need mingw-w64 installed:
# macOS: brew install mingw-w64
# Ubuntu: sudo apt install mingw-w64
```

##### 🐧 Linux
```bash
# 64-bit Linux
cargo build --release --target x86_64-unknown-linux-gnu

# ARM64 Linux (Raspberry Pi 4, etc.)
cargo build --release --target aarch64-unknown-linux-gnu

# ARM7 Linux (older Raspberry Pi)
cargo build --release --target armv7-unknown-linux-gnueabihf
```

#### 3. Using Cross for Easy Cross-Compilation
Install the `cross` tool for easier cross-compilation:

```bash
# Install cross
cargo install cross

# Build for different platforms using cross
cross build --release --target x86_64-pc-windows-gnu
cross build --release --target x86_64-unknown-linux-gnu
cross build --release --target aarch64-unknown-linux-gnu
```

#### 4. Automated Build Script
Create a build script to generate all binaries:

```bash
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

# Windows 64-bit
echo "Building for Windows..."
cross build --release --target x86_64-pc-windows-gnu
cp target/x86_64-pc-windows-gnu/release/filepilot.exe dist/filepilot-windows.exe

# Linux 64-bit
echo "Building for Linux..."
cross build --release --target x86_64-unknown-linux-gnu
cp target/x86_64-unknown-linux-gnu/release/filepilot dist/filepilot-linux

# Linux ARM64
echo "Building for Linux ARM64..."
cross build --release --target aarch64-unknown-linux-gnu
cp target/aarch64-unknown-linux-gnu/release/filepilot dist/filepilot-linux-arm64

echo "All builds complete! Check the 'dist' directory."
```

Make it executable and run:
```bash
chmod +x build-all.sh
./build-all.sh
```

### 📋 Binary Output Locations
After building, your binaries will be located at:

- **macOS Intel**: `target/x86_64-apple-darwin/release/filepilot`
- **macOS Apple Silicon**: `target/aarch64-apple-darwin/release/filepilot`
- **Windows**: `target/x86_64-pc-windows-gnu/release/filepilot.exe`
- **Linux**: `target/x86_64-unknown-linux-gnu/release/filepilot`
- **Linux ARM64**: `target/aarch64-unknown-linux-gnu/release/filepilot`

### 📦 Creating Distribution Packages

#### For macOS - Create .app Bundle
```bash
# Create app structure
mkdir -p FilePilot.app/Contents/MacOS
mkdir -p FilePilot.app/Contents/Resources

# Copy binary
cp target/release/filepilot FilePilot.app/Contents/MacOS/

# Create Info.plist
cat > FilePilot.app/Contents/Info.plist << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>filepilot</string>
    <key>CFBundleIdentifier</key>
    <string>com.yourname.filepilot</string>
    <key>CFBundleName</key>
    <string>FilePilot</string>
    <key>CFBundleVersion</key>
    <string>1.0</string>
</dict>
</plist>
EOF
```

#### For Windows - Create MSI Installer
Use tools like `cargo-wix`:
```bash
cargo install cargo-wix
cargo wix --no-build --nocapture
```

#### For Linux - Create .deb Package
Use `cargo-deb`:
```bash
cargo install cargo-deb
cargo deb
```

## 🎮 Usage

### Terminal Interface
- **↑/↓ or j/k**: Navigate files
- **Enter**: Enter directory or open file
- **Backspace**: Go to parent directory
- **S**: Share current file (URL copied to clipboard)
- **Q**: Quit application
- **/**: Search files
- **Tab**: Switch between panels

### File Sharing
1. Navigate to any file using the terminal interface
2. Press **'S'** to share the file
3. The sharing URL is automatically copied to your clipboard
4. Share the URL with anyone on your network
5. Files are viewed directly in the browser with proper formatting

### Web Interface Features
- **Direct viewing** of 25+ file types
- **Syntax highlighting** for code files
- **JSON/XML formatting** with proper indentation
- **Markdown rendering** with GitHub styling
- **Video/audio streaming** with full browser controls
- **Spreadsheet tables** with sorting and filtering
- **Jupyter notebook** rendering with cell outputs
- **Download options** always available

## 🛠️ Development

### Project Structure
```
FilePilot/
├── src/
│   ├── main.rs              # Application entry point
│   ├── ui.rs                # Terminal UI components
│   ├── file_sharing.rs      # Web server and file serving
│   ├── file_system.rs       # File system operations
│   └── search.rs            # File search functionality
├── Cargo.toml               # Dependencies and metadata
└── README.md                # This file
```

### Building for Development
```bash
# Debug build (faster compilation)
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Check for issues
cargo check
```

### Dependencies
- **crossterm** - Cross-platform terminal manipulation
- **ratatui** - Terminal UI framework
- **tokio** - Async runtime
- **warp** - Web server framework
- **uuid** - Unique file identifiers
- **serde_json** - JSON parsing and formatting
- **csv** - CSV file parsing
- **calamine** - Excel file reading
- **regex** - Pattern matching for text processing
- **arboard** - Clipboard integration
- **local-ip-address** - Network IP detection

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📄 License

This project is licensed under the GPL-3.0 License - see the [LICENSE](LICENSE) file for details.

## 🎯 Platform Support

| Platform | Architecture | Status | Binary Name |
|----------|-------------|--------|-------------|
| 🍎 macOS | Intel (x64) | ✅ Supported | `filepilot-macos-intel` |
| 🍎 macOS | Apple Silicon (ARM64) | ✅ Supported | `filepilot-macos-arm64` |
| 🪟 Windows | x64 | ✅ Supported | `filepilot-windows.exe` |
| 🐧 Linux | x64 | ✅ Supported | `filepilot-linux` |
| 🐧 Linux | ARM64 | ✅ Supported | `filepilot-linux-arm64` |
| 🐧 Linux | ARMv7 | ✅ Supported | `filepilot-linux-armv7` |

## 🚀 Performance

- **Startup time**: < 100ms
- **Memory usage**: ~10MB baseline
- **File sharing**: Instant URL generation
- **Large files**: Handles 100MB+ files efficiently
- **Concurrent users**: Supports multiple simultaneous file viewers
- **Network**: HTTP range requests for efficient video streaming

## 📸 Screenshots

### Terminal Interface
The FilePilot terminal interface provides intuitive file navigation with vim-like keybindings.

### Web File Viewer
Share any file instantly and view it in a professional dark-themed web interface with syntax highlighting, table rendering, and multimedia support.

---
