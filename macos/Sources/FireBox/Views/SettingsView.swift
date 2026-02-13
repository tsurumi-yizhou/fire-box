// SPDX-License-Identifier: Apache-2.0
// Fire Box — macOS Native Layer — Settings View
//
// Service configuration, status, and control panel.

import SwiftUI

struct SettingsView: View {
    @Bindable var state: FireBoxState

    var body: some View {
        Form {
            // ── Service ─────────────────────────────────────────
            Section("Service") {
                LabeledContent("Status") {
                    HStack(spacing: 6) {
                        Circle()
                            .fill(state.isConnected ? .green : .red)
                            .frame(width: 8, height: 8)
                        Text(state.isConnected ? "Running" : "Stopped")
                    }
                }
                LabeledContent("Socket") {
                    Text(coreSocketPath)
                        .font(.caption.monospaced())
                        .textSelection(.enabled)
                }
            }

            // ── Actions ─────────────────────────────────────────
            Section("Actions") {
                HStack(spacing: 12) {
                    Button("Start Service") {
                        state.startService()
                    }
                    .disabled(state.isConnected)

                    Button("Stop Service") {
                        state.stopService()
                    }
                    .disabled(!state.isConnected)

                    Button("Reload Config") {
                        let _ = fire_box_reload()
                        Task { await state.refresh() }
                    }
                    .disabled(!state.isConnected)
                }
            }

            // ── Error ───────────────────────────────────────────
            if let error = state.lastError {
                Section("Last Error") {
                    HStack {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundStyle(.yellow)
                        Text(error)
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }
                }
            }

            // ── About ───────────────────────────────────────────
            Section("About") {
                LabeledContent("Version", value: "0.3.0")
                LabeledContent("Architecture") {
                    #if arch(arm64)
                        Text("Apple Silicon (arm64)")
                    #else
                        Text("Intel (x86_64)")
                    #endif
                }
            }
        }
        .formStyle(.grouped)
    }
}
