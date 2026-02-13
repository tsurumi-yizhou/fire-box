// SPDX-License-Identifier: Apache-2.0
// Fire Box — macOS Native Layer — Dashboard / Overview View
//
// Shown as the "Overview" page in the main window sidebar.
// Displays a status header and embeds MetricsView for traffic data.

import SwiftUI

struct DashboardView: View {
    @Bindable var state: FireBoxState

    var body: some View {
        VStack(spacing: 0) {
            // ── Status header ───────────────────────────────────────
            statusHeader
                .padding(16)

            Divider()

            // ── Metrics content ─────────────────────────────────────
            MetricsView(state: state)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    // MARK: Subviews

    private var statusHeader: some View {
        HStack(spacing: 12) {
            Image(systemName: "flame.fill")
                .foregroundStyle(.orange)
                .font(.system(size: 28))

            VStack(alignment: .leading, spacing: 2) {
                Text("Fire Box")
                    .font(.title3.bold())
                HStack(spacing: 6) {
                    Circle()
                        .fill(state.isConnected ? .green : .red)
                        .frame(width: 8, height: 8)
                    Text(state.isConnected ? "Service Running" : "Service Offline")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
            }

            Spacer()

            // ── Start / Stop button ─────────────────────────────
            if state.isConnected {
                Button("Stop Service") {
                    state.stopService()
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
                .tint(.red)
            } else {
                Button("Start Service") {
                    state.startService()
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
            }
        }
    }
}
