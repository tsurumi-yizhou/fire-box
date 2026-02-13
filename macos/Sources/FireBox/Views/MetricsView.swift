// SPDX-License-Identifier: Apache-2.0
// Fire Box — macOS Native Layer — Metrics View
//
// Real-time overview of gateway traffic: total tokens, requests,
// active connections, with per-provider and per-model breakdowns.

import SwiftUI

struct MetricsView: View {
    @Bindable var state: FireBoxState

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                // ── Global counters ─────────────────────────────────
                globalCounters

                Divider()

                // ── Per-provider breakdown ──────────────────────────
                if !state.metrics.providerMetrics().isEmpty {
                    sectionHeader("By Provider")
                    ForEach(state.metrics.providerMetrics()) { m in
                        entityRow(m)
                    }
                }

                Divider()

                // ── Per-model breakdown ─────────────────────────────
                if !state.metrics.modelMetrics().isEmpty {
                    sectionHeader("By Model")
                    ForEach(state.metrics.modelMetrics()) { m in
                        entityRow(m)
                    }
                }
            }
            .padding(16)
        }
    }

    // MARK: Subviews

    private var globalCounters: some View {
        LazyVGrid(
            columns: [
                GridItem(.flexible()),
                GridItem(.flexible()),
            ], spacing: 12
        ) {
            counterCard(
                title: "Requests",
                value: "\(state.metrics.totalRequests)",
                icon: "arrow.up.arrow.down",
                color: .blue)
            counterCard(
                title: "Active",
                value: "\(state.metrics.activeConnections)",
                icon: "link",
                color: .green)
            counterCard(
                title: "Input Tok",
                value: formatTokens(state.metrics.totalInputTokens),
                icon: "arrow.right.circle",
                color: .orange)
            counterCard(
                title: "Output Tok",
                value: formatTokens(state.metrics.totalOutputTokens),
                icon: "arrow.left.circle",
                color: .purple)
        }
    }

    private func counterCard(title: String, value: String, icon: String, color: Color) -> some View
    {
        VStack(spacing: 4) {
            HStack {
                Image(systemName: icon)
                    .foregroundStyle(color)
                    .font(.caption)
                Spacer()
            }
            HStack {
                Text(value)
                    .font(.title2.monospacedDigit().bold())
                Spacer()
            }
            HStack {
                Text(title)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
            }
        }
        .padding(10)
        .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 8))
    }

    private func sectionHeader(_ title: String) -> some View {
        Text(title)
            .font(.subheadline.bold())
            .foregroundStyle(.secondary)
    }

    private func entityRow(_ m: EntityMetrics) -> some View {
        HStack {
            Text(m.name)
                .font(.body)
                .lineLimit(1)
            Spacer()
            Group {
                Label("\(m.requests)", systemImage: "number")
                Label(formatTokens(m.inputTokens), systemImage: "arrow.right")
                Label(formatTokens(m.outputTokens), systemImage: "arrow.left")
            }
            .font(.caption.monospacedDigit())
            .foregroundStyle(.secondary)
        }
        .padding(.vertical, 2)
    }

    // MARK: Helpers

    private func formatTokens(_ n: UInt64) -> String {
        switch n {
        case 0..<1_000: return "\(n)"
        case 1_000..<1_000_000: return String(format: "%.1fK", Double(n) / 1_000)
        default: return String(format: "%.1fM", Double(n) / 1_000_000)
        }
    }
}
