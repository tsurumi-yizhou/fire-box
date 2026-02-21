import SwiftUI
import Charts

struct DashboardView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                Text("Dashboard")
                    .font(.largeTitle)
                    .bold()
                    .padding(.horizontal)

                if let metrics = appState.metrics {
                    MetricsCardsView(metrics: metrics)
                        .padding(.horizontal)

                    Divider()
                        .padding(.vertical)

                    RecentActivityView()
                        .padding(.horizontal)
                } else {
                    ProgressView("Loading metrics...")
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .padding(.vertical)
        }
    }
}

struct MetricsCardsView: View {
    let metrics: MetricsSnapshot

    var body: some View {
        LazyVGrid(columns: [
            GridItem(.flexible()),
            GridItem(.flexible()),
            GridItem(.flexible())
        ], spacing: 16) {
            MetricCard(
                title: "Total Requests",
                value: "\(metrics.totalRequests)",
                icon: "arrow.up.arrow.down.circle.fill",
                color: .blue
            )

            MetricCard(
                title: "Input Tokens",
                value: formatNumber(metrics.totalTokensInput),
                icon: "arrow.down.circle.fill",
                color: .green
            )

            MetricCard(
                title: "Output Tokens",
                value: formatNumber(metrics.totalTokensOutput),
                icon: "arrow.up.circle.fill",
                color: .orange
            )

            MetricCard(
                title: "Total Cost",
                value: String(format: "$%.2f", metrics.totalCost),
                icon: "dollarsign.circle.fill",
                color: .purple
            )

            MetricCard(
                title: "Active Connections",
                value: "\(metrics.activeConnections)",
                icon: "network",
                color: .cyan
            )

            MetricCard(
                title: "Avg Cost/Request",
                value: String(format: "$%.4f", metrics.totalRequests > 0 ? metrics.totalCost / Double(metrics.totalRequests) : 0),
                icon: "chart.line.uptrend.xyaxis.circle.fill",
                color: .pink
            )
        }
    }

    private func formatNumber(_ number: Int64) -> String {
        let formatter = NumberFormatter()
        formatter.numberStyle = .decimal
        return formatter.string(from: NSNumber(value: number)) ?? "\(number)"
    }
}

struct MetricCard: View {
    let title: String
    let value: String
    let icon: String
    let color: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Image(systemName: icon)
                    .foregroundColor(color)
                    .font(.title2)
                Spacer()
            }

            Text(value)
                .font(.title)
                .bold()

            Text(title)
                .font(.caption)
                .foregroundColor(.secondary)
        }
        .padding()
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .controlBackgroundColor))
        .cornerRadius(12)
    }
}

struct RecentActivityView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Recent Activity")
                .font(.title2)
                .bold()

            if appState.connections.isEmpty {
                Text("No active connections")
                    .foregroundColor(.secondary)
                    .padding()
            } else {
                ForEach(appState.connections.prefix(5)) { connection in
                    HStack {
                        Image(systemName: "app.fill")
                            .foregroundColor(.blue)

                        VStack(alignment: .leading, spacing: 4) {
                            Text(connection.programName)
                                .font(.headline)
                            Text("\(connection.requestCount) requests")
                                .font(.caption)
                                .foregroundColor(.secondary)
                        }

                        Spacer()

                        Text(timeAgo(from: connection.lastActivityMs))
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                    .padding()
                    .background(Color(nsColor: .controlBackgroundColor))
                    .cornerRadius(8)
                }
            }
        }
    }

    private func timeAgo(from timestampMs: Int64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(timestampMs) / 1000)
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .abbreviated
        return formatter.localizedString(for: date, relativeTo: Date())
    }
}
