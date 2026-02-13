// SPDX-License-Identifier: Apache-2.0
// Fire Box — macOS Native Layer — Application Delegate
//
// Lifecycle management: Dock icon, menu-bar status item, core service.
// GUI shows first; core starts in the background afterwards.

import AppKit
import SwiftUI

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let state = FireBoxState()
    private var statusBarController: StatusBarController?

    // MARK: NSApplicationDelegate

    func applicationDidFinishLaunching(_: Notification) {
        // ── Show in Dock ────────────────────────────────────────────────
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)

        // ── Configure IPC client & start polling immediately ────────────
        CoreClient.shared.configure(socketPath: coreSocketPath)
        state.start()

        // ── Menu-bar status item ────────────────────────────────────────
        statusBarController = StatusBarController(state: state)

        // ── XPC service ─────────────────────────────────────────────────
        XPCService.shared.start()

        // ── Start core service in background (after GUI is shown) ───────
        state.startService()
    }

    func applicationWillTerminate(_: Notification) {
        state.stop()
    }

    /// Don't quit when the last window closes — keep running in menu bar.
    func applicationShouldTerminateAfterLastWindowClosed(_: NSApplication) -> Bool {
        false
    }

    /// Re-show the main window when the Dock icon is clicked.
    func applicationShouldHandleReopen(_: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        if !flag {
            if NSApp.activationPolicy() == .accessory {
                NSApp.setActivationPolicy(.regular)
                NSRunningApplication.runningApplications(
                    withBundleIdentifier: "com.apple.dock"
                ).first?.activate()
            }
            for window in NSApp.windows where !window.title.isEmpty {
                window.makeKeyAndOrderFront(nil)
            }
            NSApp.activate(ignoringOtherApps: true)
        }
        return true
    }
}
