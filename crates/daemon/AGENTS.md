# crates/daemon/

> ⚠️ **Please update me promptly.**

`fire-box` binary crate — daemon entrypoint.

## Responsibilities

Minimal wrapper: `main()` calls `fire_box_core::run_from_args()`; no special arguments. All core logic lives in `fire-box-core`.

## Subdirectories

| Directory | Description |
|-----------|-------------|
| `src/`    | Binary source code |
