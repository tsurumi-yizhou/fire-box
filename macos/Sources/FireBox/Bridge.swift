// SPDX-License-Identifier: Apache-2.0
// Fire Box — macOS Native Layer — Application Entry Point
//
// SwiftUI app with a main Window (NavigationSplitView).
// The Rust core is started in the background *after* the GUI is shown.

import SwiftUI

// Import the C header with Rust core functions
#if canImport(Darwin)
    import Darwin
#endif

// C function declarations
@_silgen_name("fire_box_start")
func fire_box_start() -> Int32

@_silgen_name("fire_box_stop")
func fire_box_stop() -> Int32

@_silgen_name("fire_box_reload")
func fire_box_reload() -> Int32

/// Socket path for communicating with the Rust core IPC server.
nonisolated(unsafe) var coreSocketPath: String = "/tmp/fire-box-ipc.sock"

@main
struct FireBoxApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate

    var body: some Scene {
        Window("Fire Box", id: "main") {
            MainView(state: appDelegate.state)
        }
        .defaultSize(width: 800, height: 560)
        .commands {
            SidebarCommands()
            CommandGroup(replacing: .appTermination) {
                Button("Quit Fire Box") {
                    let _ = fire_box_stop()
                    NSApplication.shared.terminate(nil)
                }
                .keyboardShortcut("q", modifiers: [.command])
            }
        }
    }
}
