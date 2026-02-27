# Fire Box

A high-performance, cross-platform AI gateway written in Rust. It supports multiple protocols, including OpenAI, Anthropic, DashScope.

[![Linux](https://github.com/tsurumi-yizhou/fire-box/actions/workflows/linux.yml/badge.svg)](https://github.com/tsurumi-yizhou/fire-box/actions/workflows/linux.yml)
[![macOS](https://github.com/tsurumi-yizhou/fire-box/actions/workflows/macos.yml/badge.svg)](https://github.com/tsurumi-yizhou/fire-box/actions/workflows/macos.yml)
[![Windows](https://github.com/tsurumi-yizhou/fire-box/actions/workflows/windows.yml/badge.svg)](https://github.com/tsurumi-yizhou/fire-box/actions/workflows/windows.yml)

## Requirements

### Build Dependencies

- **CMake** 3.20 or later
- **Rust** 1.70 or later (with cargo)
- **Platform-specific:**
  - macOS: Xcode Command Line Tools, Swift 6.1+
  - Windows: Visual Studio 2019+ or MSVC build tools
  - Linux: GCC/Clang, pkg-config

### Runtime Dependencies

- **macOS:** macOS 15.0 or later
- **Windows:** Windows 10 or later
- **Linux:** glibc 2.31+, xdg-utils (for URL scheme registration)

## Quick Start

### Building from Source

```bash
# Clone the repository
git clone https://github.com/tsurumi-yizhou/fire-box.git
cd fire-box

# Configure with CMake
cmake -B build

# Build
cmake --build build

# Create installation package
cd build
cpack
```

### Installing

**macOS:**
```bash
# Install the generated .pkg
sudo installer -pkg FireBox-1.1.0-macOS.pkg -target /
```

**Windows:**
```powershell
# Run the generated installer
.\FireBox-1.1.0-Windows.exe
```

**Linux:**
```bash
# Install the generated .deb package
sudo dpkg -i firebox_1.1.0_amd64.deb
```

### Using URL Scheme

After installation, you can configure providers using `firebox://` URLs:

```
firebox://add-provider?type=openai&name=OpenAI&config=eyJiYXNlX3VybCI6Imh0dHBzOi8vYXBpLm9wZW5haS5jb20vdjEifQ==
```

The application will prompt for confirmation before adding the provider.