// SPDX-License-Identifier: Apache-2.0
// Fire Box — macOS Native Layer — OAuth Prompt View
//
// Shown as a sheet when the core needs the user to complete an
// OAuth device-code flow in a browser.

import SwiftUI

struct OAuthPromptView: View {
    let prompt: FireBoxState.OAuthPrompt

    @Environment(\.dismiss) private var dismiss
    @State private var copied = false

    var body: some View {
        VStack(spacing: 20) {
            Image(systemName: "person.badge.key")
                .font(.system(size: 48))
                .foregroundStyle(.blue)

            Text("OAuth Authorization")
                .font(.title3.bold())

            Text("Provider **\(prompt.provider)** requires browser authorization.")
                .font(.callout)
                .multilineTextAlignment(.center)

            // ── User code ───────────────────────────────────────────
            VStack(spacing: 8) {
                Text("Your code:")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text(prompt.userCode)
                    .font(.title.monospaced().bold())
                    .textSelection(.enabled)
                Button(copied ? "Copied!" : "Copy Code") {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(prompt.userCode, forType: .string)
                    copied = true
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }
            .padding()
            .frame(maxWidth: .infinity)
            .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 8))

            // ── Open browser ────────────────────────────────────────
            Button("Open in Browser") {
                if let url = URL(string: prompt.url) {
                    NSWorkspace.shared.open(url)
                }
            }
            .buttonStyle(.borderedProminent)

            Button("Dismiss") { dismiss() }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
        }
        .padding(24)
        .frame(width: 360)
    }
}
