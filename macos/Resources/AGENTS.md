# macos/Resources/

> ⚠️ **Please update me promptly.**

Runtime resources for the macOS native layer.

## Files

| File | Description |
|------|-------------|
| `com.firebox.agent.plist` | launchd user agent property list. Configures `com.firebox.agent` to run at login, registers `com.firebox.xpc` Mach service, logs to `/tmp/fire-box.{stdout,stderr}.log` |
