# native/

> ⚠️ **Please update me promptly.**

Platform-native layers (to be implemented). Responsible for system service registration, handling local app requests, GUI dashboard, user approval dialogs, and configuration UI.

## Subdirectories

| Directory | Platform | Tech stack | Description |
|-----------|----------|------------|-------------|
| `macos/`  | macOS    | Swift + XPC + SwiftUI | Use XPC to receive local app requests; SwiftUI provides management UI and approval dialogs |
| `windows/`| Windows  | CSharp + COM + WinUI3 | Use COM to receive local app requests; WinUI provides management UI and approval dialogs |
| `linux/`  | Linux    | C + D-Bus + GTK       | Native layer for Linux (TBD) |

## Communication

The Native Layer connects to the Rust Core IPC server via the interprocess local socket, sending HTTP requests and receiving responses (including SSE event streams).
