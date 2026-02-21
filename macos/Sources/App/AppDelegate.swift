import AppKit
import SwiftUI

@MainActor
class AppDelegate: NSObject, NSApplicationDelegate {
    private var statusBarItem: NSStatusItem?
    private var serviceClient: ServiceClient?

    func applicationDidFinishLaunching(_ notification: Notification) {
        setupStatusBarItem()
        serviceClient = ServiceClient.shared

        // Hide dock icon, show only in menu bar
        NSApp.setActivationPolicy(.accessory)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        // Keep app running in menu bar even when window is closed
        return false
    }

    private func setupStatusBarItem() {
        statusBarItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)

        if let button = statusBarItem?.button {
            button.image = NSImage(systemSymbolName: "flame.fill", accessibilityDescription: "Firebox")
        }

        let menu = NSMenu()

        menu.addItem(NSMenuItem(title: "Show Firebox", action: #selector(showMainWindow), keyEquivalent: ""))
        menu.addItem(NSMenuItem.separator())

        let statusMenuItem = NSMenuItem(title: "Service: Unknown", action: nil, keyEquivalent: "")
        statusMenuItem.tag = 100
        menu.addItem(statusMenuItem)

        menu.addItem(NSMenuItem.separator())
        menu.addItem(NSMenuItem(title: "Quit Firebox", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))

        self.statusBarItem?.menu = menu

        // Update service status periodically
        Timer.scheduledTimer(withTimeInterval: 5.0, repeats: true) { [weak self] _ in
            Task { @MainActor in
                self?.updateServiceStatus()
            }
        }
    }

    @objc private func showMainWindow() {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)

        if let window = NSApp.windows.first {
            window.makeKeyAndOrderFront(nil)
        }
    }

    private func updateServiceStatus() {
        guard let menu = statusBarItem?.menu,
              let statusMenuItem = menu.item(withTag: 100) else { return }

        Task {
            let isRunning = await serviceClient?.checkServiceStatus() ?? false
            await MainActor.run {
                statusMenuItem.title = isRunning ? "Service: Running" : "Service: Stopped"
            }
        }
    }
}
