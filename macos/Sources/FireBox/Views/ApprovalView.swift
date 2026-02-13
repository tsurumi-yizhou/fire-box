// SPDX-License-Identifier: Apache-2.0
// Fire Box — macOS Native Layer — Approval View
//
// Shown as a modal sheet when the core emits an `auth_required` event.
// Displays the requesting app's identity and lets the user approve or deny.

import SwiftUI

struct ApprovalView: View {
    @Bindable var state: FireBoxState
    let approval: FireBoxState.PendingApproval

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 20) {
            // ── Icon ────────────────────────────────────────────────
            Image(systemName: "lock.shield")
                .font(.system(size: 48))
                .foregroundStyle(.orange)

            // ── Title ───────────────────────────────────────────────
            Text("Authorization Request")
                .font(.title3.bold())

            // ── Info ────────────────────────────────────────────────
            VStack(alignment: .leading, spacing: 8) {
                infoRow(label: "App", value: approval.appName)
                infoRow(label: "ID", value: approval.appId)
                if !approval.requestedModels.isEmpty {
                    infoRow(
                        label: "Models",
                        value: approval.requestedModels.joined(separator: ", "))
                }
            }
            .padding()
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 8))

            Text("This app wants to use Fire Box to access AI models. Allow?")
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

            // ── Buttons ─────────────────────────────────────────────
            HStack(spacing: 16) {
                Button("Deny") {
                    Task {
                        await state.deny(requestId: approval.id, appId: approval.appId)
                        dismiss()
                    }
                }
                .keyboardShortcut(.cancelAction)

                Button("Approve") {
                    Task {
                        await state.approve(
                            requestId: approval.id,
                            appId: approval.appId,
                            models: approval.requestedModels)
                        dismiss()
                    }
                }
                .keyboardShortcut(.defaultAction)
                .buttonStyle(.borderedProminent)
            }
        }
        .padding(24)
        .frame(width: 380)
    }

    private func infoRow(label: String, value: String) -> some View {
        HStack(alignment: .top) {
            Text(label)
                .font(.caption.bold())
                .foregroundStyle(.secondary)
                .frame(width: 50, alignment: .trailing)
            Text(value)
                .font(.caption)
                .textSelection(.enabled)
        }
    }
}
