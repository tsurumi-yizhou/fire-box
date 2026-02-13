// SPDX-License-Identifier: Apache-2.0
// Fire Box — macOS Native Layer — Providers View
//
// Displays configured LLM providers and their current status.

import SwiftUI

struct ProvidersView: View {
    @Bindable var state: FireBoxState

    var body: some View {
        Group {
            if state.providers.isEmpty {
                ContentUnavailableView(
                    "No Providers",
                    systemImage: "cloud.slash",
                    description: Text("Configure providers through the core's IPC API.")
                )
            } else {
                List {
                    ForEach(state.providers) { prov in
                        providerRow(prov)
                    }
                }
                .listStyle(.plain)
            }
        }
    }

    private func providerRow(_ prov: ProviderInfo) -> some View {
        HStack {
            Image(systemName: symbolForType(prov.type))
                .foregroundStyle(colorForType(prov.type))
                .frame(width: 24)

            VStack(alignment: .leading, spacing: 2) {
                Text(prov.tag)
                    .font(.body.bold())
                HStack(spacing: 8) {
                    Text(prov.type)
                        .font(.caption)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 1)
                        .background(.quaternary, in: Capsule())
                    if let url = prov.baseUrl {
                        Text(url)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
            }

            Spacer()

            // Per-provider metrics (if available)
            let metrics = state.metrics.perProvider[prov.tag]
            if let m = metrics {
                VStack(alignment: .trailing, spacing: 2) {
                    Text("\(m.requests) req")
                        .font(.caption.monospacedDigit())
                    Text("\(m.inputTokens + m.outputTokens) tok")
                        .font(.caption2.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
            }
        }
        .padding(.vertical, 4)
    }

    // MARK: Helpers

    private func symbolForType(_ type: String) -> String {
        switch type.lowercased() {
        case "openai": return "brain.head.profile"
        case "anthropic": return "person.bust"
        case "dashscope": return "cloud.sun"
        case "copilot": return "chevron.left.forwardslash.chevron.right"
        default: return "cloud"
        }
    }

    private func colorForType(_ type: String) -> Color {
        switch type.lowercased() {
        case "openai": return .green
        case "anthropic": return .orange
        case "dashscope": return .blue
        case "copilot": return .purple
        default: return .gray
        }
    }
}
