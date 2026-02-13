// SPDX-License-Identifier: Apache-2.0
// Fire Box — macOS Native Layer — Status Bar Controller
//
// Manages the NSStatusItem (menu-bar icon) with a context menu.
// Provides "Open", "Start/Stop Service", and "Quit" actions.

import AppKit

@MainActor
final class StatusBarController: NSObject, @preconcurrency NSMenuDelegate {
    private var statusItem: NSStatusItem?
    private var menu: NSMenu?
    private let state: FireBoxState

    init(state: FireBoxState) {
        self.state = state
        super.init()
        createStatusItem()
    }

    // MARK: Status Item

    private func createStatusItem() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        guard let button = statusItem?.button else { return }

        button.image = NSImage(
            systemSymbolName: "flame.fill",
            accessibilityDescription: "Fire Box"
        )

        menu = NSMenu()
        menu?.delegate = self
        statusItem?.menu = menu
        rebuildMenu()
    }

    private func rebuildMenu() {
        guard let menu else { return }
        menu.removeAllItems()

        // ── Status ──────────────────────────────────────────────
        let statusTitle = state.isConnected ? "● Service Running" : "○ Service Stopped"
        let statusMenuItem = NSMenuItem(title: statusTitle, action: nil, keyEquivalent: "")
        statusMenuItem.isEnabled = false
        menu.addItem(statusMenuItem)

        menu.addItem(NSMenuItem.separator())

        // ── Open ────────────────────────────────────────────────
        let openItem = NSMenuItem(
            title: "Open Fire Box",
            action: #selector(openApp),
            keyEquivalent: ""
        )
        openItem.target = self
        menu.addItem(openItem)

        menu.addItem(NSMenuItem.separator())

        // ── Start / Stop ────────────────────────────────────────
        if state.isConnected {
            let stopItem = NSMenuItem(
                title: "Stop Service",
                action: #selector(stopService),
                keyEquivalent: ""
            )
            stopItem.target = self
            menu.addItem(stopItem)
        } else {
            let startItem = NSMenuItem(
                title: "Start Service",
                action: #selector(startService),
                keyEquivalent: ""
            )
            startItem.target = self
            menu.addItem(startItem)
        }

        menu.addItem(NSMenuItem.separator())

        // ── Quit ────────────────────────────────────────────────
        let quitItem = NSMenuItem(
            title: "Quit Fire Box",
            action: #selector(quitApp),
            keyEquivalent: "q"
        )
        quitItem.target = self
        menu.addItem(quitItem)
    }

    // MARK: Actions

    @objc private func openApp() {
        NSApp.setActivationPolicy(.regular)
        for window in NSApp.windows where !window.title.isEmpty {
            window.makeKeyAndOrderFront(nil)
        }
        NSApp.activate(ignoringOtherApps: true)
    }

    @objc private func startService() {
        state.startService()
    }

    @objc private func stopService() {
        state.stopService()
    }

    @objc private func quitApp() {
        state.stopService()
        NSApplication.shared.terminate(nil)
    }

    // MARK: NSMenuDelegate

    nonisolated func menuWillOpen(_ menu: NSMenu) {
        MainActor.assumeIsolated {
            self.rebuildMenu()
        }
    }
}
