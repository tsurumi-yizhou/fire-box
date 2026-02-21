import SwiftUI

struct ConnectionsView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Connections")
                    .font(.largeTitle)
                    .bold()

                Spacer()

                Text("\(appState.connections.count) active")
                    .foregroundColor(.secondary)
            }
            .padding()

            Divider()

            if appState.connections.isEmpty {
                VStack(spacing: 12) {
                    Image(systemName: "network.slash")
                        .font(.system(size: 48))
                        .foregroundColor(.secondary)
                    Text("No active connections")
                        .font(.title3)
                        .foregroundColor(.secondary)
                    Text("Local programs will appear here when they connect")
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List {
                    ForEach(appState.connections) { connection in
                        ConnectionRow(connection: connection)
                    }
                }
                .listStyle(.inset)
            }
        }
    }
}

struct ConnectionRow: View {
    let connection: Connection

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Image(systemName: "app.fill")
                    .font(.title2)
                    .foregroundColor(.blue)

                VStack(alignment: .leading, spacing: 4) {
                    Text(connection.programName)
                        .font(.headline)

                    Text(connection.programPath)
                        .font(.caption)
                        .foregroundColor(.secondary)
                        .lineLimit(1)
                }

                Spacer()

                VStack(alignment: .trailing, spacing: 4) {
                    HStack(spacing: 4) {
                        Image(systemName: "circle.fill")
                            .font(.system(size: 8))
                            .foregroundColor(.green)
                        Text("Active")
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }

                    Text("\(connection.requestCount) requests")
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
            }

            HStack(spacing: 16) {
                InfoLabel(
                    icon: "clock",
                    text: "Connected: \(formatDate(connection.connectedAtMs))"
                )

                InfoLabel(
                    icon: "clock.arrow.circlepath",
                    text: "Last activity: \(timeAgo(from: connection.lastActivityMs))"
                )
            }
            .font(.caption)
            .foregroundColor(.secondary)
        }
        .padding(.vertical, 8)
    }

    private func formatDate(_ timestampMs: Int64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(timestampMs) / 1000)
        let formatter = DateFormatter()
        formatter.dateStyle = .short
        formatter.timeStyle = .short
        return formatter.string(from: date)
    }

    private func timeAgo(from timestampMs: Int64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(timestampMs) / 1000)
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .abbreviated
        return formatter.localizedString(for: date, relativeTo: Date())
    }
}

struct InfoLabel: View {
    let icon: String
    let text: String

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: icon)
            Text(text)
        }
    }
}
