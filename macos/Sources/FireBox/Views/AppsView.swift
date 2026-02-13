// SPDX-License-Identifier: Apache-2.0
// Fire Box — macOS Native Layer — Apps View
//
// Lists all registered (authorized / pending) apps with their usage
// stats and a Revoke button.

import SwiftUI

struct AppsView: View {
    @Bindable var state: FireBoxState

    var body: some View {
        Group {
            if state.apps.isEmpty {
                ContentUnavailableView(
                    "No Apps Registered",
                    systemImage: "app.dashed",
                    description: Text("Apps will appear here when they connect via XPC.")
                )
            } else {
                List {
                    ForEach(state.apps) { app in
                        appRow(app)
                    }
                }
                .listStyle(.plain)
            }
        }
    }

    private func appRow(_ app: AppInfo) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Circle()
                        .fill(app.authorized ? .green : .red)
                        .frame(width: 6, height: 6)
                    Text(app.appName)
                        .font(.body.bold())
                        .lineLimit(1)
                }
                Text(app.appId)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)

                HStack(spacing: 12) {
                    Label("\(app.totalRequests) req", systemImage: "number")
                    if !app.allowedModels.isEmpty {
                        Label(
                            app.allowedModels.joined(separator: ", "),
                            systemImage: "cpu"
                        )
                        .lineLimit(1)
                    }
                }
                .font(.caption2)
                .foregroundStyle(.tertiary)
            }

            Spacer()

            if app.authorized {
                Button("Revoke") {
                    Task { await state.revokeApp(id: app.appId) }
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
                .tint(.red)
            }
        }
        .padding(.vertical, 4)
    }
}
