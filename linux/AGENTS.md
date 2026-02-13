# linux/

> ⚠️ **Please update me promptly.**

Linux native layer — C daemon + GTK4/libadwaita GUI, built with Meson.

## Architecture

Two executables:

| Program | Description |
|---------|-------------|
| `daemon` | systemd-compatible service, links Rust core (`libcore.a`), calls `fire_box_start()`, handles SIGTERM/SIGHUP |
| `main` | Desktop GUI (GTK4 / libadwaita / ayatana-appindicator), placeholder |

The Rust core (`libcore.a`) is linked as a static library into the daemon. Signal handlers manage shutdown (SIGTERM/SIGINT → `fire_box_stop()`) and configuration reload (SIGHUP → `fire_box_reload()`). The daemon logs via syslog.

## Build

```sh
# 1. Build the Rust static library (produces generated/libcore.a + generated/core.h)
cd core && cargo build

# 2. Build the C executables (Meson)
cd linux && meson setup builddir && meson compile -C builddir
```

## Dependencies

- **Daemon only:** libc, pthreads (no extra deps)
- **GUI:** `libgtk-4-dev`, `libadwaita-1-dev`, `libayatana-appindicator3-dev`

## Subdirectories

| Path | Description |
|------|-------------|
| `src/daemon.c` | Daemon entry point — signal handling, core lifecycle, syslog |
| `src/main.c` | GUI entry point (placeholder) |
| `include/` | Local C headers (currently empty) |

## Key files

| File | Description |
|------|-------------|
| `meson.build` | Meson build definition. `daemon` links `../generated/libcore.a`; `main` links GTK4/libadwaita (optional deps) |
