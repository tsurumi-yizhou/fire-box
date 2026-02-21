# Fire Box Build

export RUST_BACKTRACE := "1"
set shell := ["bash", "-cu"]

default:
    @just --list

build-macos:
    cargo build --workspace && cd macos && swift build

build-linux:
    #!/usr/bin/env bash
    # Clean macOS metadata files locally
    find . -name '._*' -delete 2>/dev/null || true
    find . -name '.DS_Store' -delete 2>/dev/null || true
    # Pack necessary source files
    tar czf - Cargo.toml client service linux \
      | ssh Linux "cd ~/tmp/fire-box && rm -rf Cargo.toml client service linux/ui linux/src linux/meson.build linux/resources linux/po && tar xzf -"
    ssh Linux "cd ~/tmp/fire-box && cargo build --workspace && cd linux && meson setup build --reconfigure && meson compile -C build"

build-windows:
    #!/usr/bin/env bash
    # Clean macOS metadata files locally
    find . -name '._*' -delete
    find . -name '.DS_Store' -delete
    # Copy necessary source files
    rm -rf /tmp/fire-box-upload && mkdir -p /tmp/fire-box-upload
    cp -r Cargo.toml client service windows /tmp/fire-box-upload/
    # Sync to Windows (clean source files but keep bin/ and obj/ build cache)
    ssh Windows 'powershell -Command "cd $env:USERPROFILE/tmp/fire-box; Remove-Item -Force Cargo.toml, windows/Firebox.slnx -ErrorAction SilentlyContinue; Remove-Item -Recurse -Force client, service -ErrorAction SilentlyContinue; Get-ChildItem windows/App -Exclude bin,obj | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue; Get-ChildItem windows/Helper -Exclude bin,obj | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue"'
    scp -r /tmp/fire-box-upload/* Windows:~/tmp/fire-box/
    ssh Windows 'cd ~/tmp/fire-box && cargo build --workspace && cd windows && dotnet build Firebox.slnx'
