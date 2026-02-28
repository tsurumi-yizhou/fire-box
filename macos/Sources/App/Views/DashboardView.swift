import SwiftUI

struct DashboardView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                HStack {
                    Text("Dashboard")
                        .font(.largeTitle)
                        .bold()

                    Spacer()

                    HStack(spacing: 6) {
                        Image(systemName: "circle.fill")
                            .font(.system(size: 8))
                            .foregroundColor(appState.serviceRunning ? .green : .red)
                        Text(appState.serviceRunning ? "Service Running" : "Service Stopped")
                            .font(.callout)
                            .foregroundColor(.secondary)
                    }
                }
                .padding(.horizontal)

                if let metrics = appState.metrics {
                    MetricsCardsView(metrics: metrics, connectionCount: appState.connections.count)
                        .padding(.horizontal)

                    Divider()
                        .padding(.vertical)

                    RecentActivityView()
                        .padding(.horizontal)
                } else {
                    VStack(spacing: 12) {
                        if appState.serviceRunning {
                            ProgressView("Loading metrics…")
                        } else {
                            Image(systemName: "exclamationmark.triangle")
                                .font(.system(size: 48))
                                .foregroundColor(.secondary)
                            Text("Service is not running")
                                .font(.title3)
                                .foregroundColor(.secondary)
                        }
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .padding(.vertical)
        }
    }
}

struct MetricsCardsView: View {
    let metrics: MetricsSnapshot
    let connectionCount: Int

    var body: some View {
        LazyVGrid(columns: [
            GridItem(.flexible()),
            GridItem(.flexible()),
            GridItem(.flexible())
        ], spacing: 16) {
            MetricCard(
                title: "Total Requests",
                value: formatNumber(metrics.requestsTotal),
                icon: "arrow.up.arrow.down.circle.fill",
                color: .blue
            )

            MetricCard(
                title: "Failed Requests",
                value: formatNumber(metrics.requestsFailed),
                icon: "xmark.circle.fill",
                color: .red
            )

            MetricCard(
                title: "Input Tokens",
                value: formatNumber(metrics.promptTokensTotal),
                icon: "arrow.down.circle.fill",
                color: .green
            )

            MetricCard(
                title: "Output Tokens",
                value: formatNumber(metrics.completionTokensTotal),
                icon: "arrow.up.circle.fill",
                color: .orange
            )

            MetricCard(
                title: "Total Cost",
                value: String(format: "$%.2f", metrics.costTotal),
                icon: "dollarsign.circle.fill",
                color: .purple
            )

            MetricCard(
                title: "Avg Latency",
                value: "\(metrics.latencyAvgMs) ms",
                icon: "clock.fill",
                color: .yellow
            )

            MetricCard(
                title: "Active Connections",
                value: "\(connectionCount)",
                icon: "network",
                color: .cyan
            )

            MetricCard(
                title: "Avg Cost/Request",
                value: String(format: "$%.4f", metrics.requestsTotal > 0 ? metrics.costTotal / Double(metrics.requestsTotal) : 0),
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
                            Text(connection.clientName)
                                .font(.headline)
                            Text("\(connection.requestsCount) requests")
                                .font(.caption)
                                .foregroundColor(.secondary)
                        }

                        Spacer()

                        Text(formatDate(connection.connectedAtMs))
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

    private func formatDate(_ timestampMs: Int64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(timestampMs) / 1000)
        let formatter = DateFormatter()
        formatter.dateStyle = .short
        formatter.timeStyle = .short
        return formatter.string(from: date)
    }
}
