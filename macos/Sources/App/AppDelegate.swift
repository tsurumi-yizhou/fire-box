import AppKit
import ServiceManagement
import SwiftUI

@MainActor
class AppDelegate: NSObject, NSApplicationDelegate {
    private var statusBarItem: NSStatusItem?

    func applicationDidFinishLaunching(_ notification: Notification) {
        setupStatusBarItem()
        registerService()

        // Hide dock icon, show only in menu bar
        NSApp.setActivationPolicy(.accessory)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }

    // MARK: - SMAppService

    private func registerService() {
        let service = SMAppService.daemon(plistName: "com.firebox.service")
        if service.status != .enabled {
            do {
                try service.register()
                NSLog("[FireBox] Service registered via SMAppService")
            } catch {
                NSLog("[FireBox] Failed to register service: %@", "\(error)")
            }
        }
    }

    // MARK: - Status Bar

    private func setupStatusBarItem() {
        statusBarItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)

        if let button = statusBarItem?.button {
            button.image = NSImage(systemSymbolName: "flame.fill", accessibilityDescription: "FireBox")
        }

        let menu = NSMenu()

        menu.addItem(NSMenuItem(title: "Show FireBox", action: #selector(showMainWindow), keyEquivalent: ""))
        menu.addItem(NSMenuItem.separator())

        let statusMenuItem = NSMenuItem(title: "Service: Checking…", action: nil, keyEquivalent: "")
        statusMenuItem.tag = 100
        menu.addItem(statusMenuItem)

        menu.addItem(NSMenuItem.separator())
        menu.addItem(NSMenuItem(title: "Quit FireBox", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))

        statusBarItem?.menu = menu

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
            let isRunning = await ServiceClient.shared.checkServiceStatus()
            await MainActor.run {
                statusMenuItem.title = isRunning ? "Service: Running" : "Service: Stopped"
            }
        }
    }
}
