import SwiftUI

struct AllowlistView: View {
    @EnvironmentObject var appState: AppState
    @State private var confirmRevoke: AllowlistEntry?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Allowlist")
                    .font(.largeTitle)
                    .bold()

                Spacer()

                Text("\(appState.allowlist.count) app(s)")
                    .foregroundColor(.secondary)
            }
            .padding()

            Divider()

            if appState.allowlist.isEmpty {
                VStack(spacing: 12) {
                    Image(systemName: "checkmark.shield")
                        .font(.system(size: 48))
                        .foregroundColor(.secondary)
                    Text("No apps in allowlist")
                        .font(.title3)
                        .foregroundColor(.secondary)
                    Text("Apps will appear here after you approve their first connection")
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List {
                    ForEach(appState.allowlist) { entry in
                        AllowlistRow(entry: entry, onRevoke: {
                            confirmRevoke = entry
                        })
                    }
                }
                .listStyle(.inset)
            }
        }
        .alert("Revoke Access", isPresented: .init(
            get: { confirmRevoke != nil },
            set: { if !$0 { confirmRevoke = nil } }
        )) {
            Button("Cancel", role: .cancel) { confirmRevoke = nil }
            Button("Revoke", role: .destructive) {
                if let entry = confirmRevoke {
                    Task {
                        _ = await ServiceClient.shared.removeFromAllowlist(appPath: entry.appPath)
                        await appState.refreshData()
                    }
                }
                confirmRevoke = nil
            }
        } message: {
            Text("Revoke access for \"\(confirmRevoke?.displayName ?? "")\"? The app will need to be re-approved on next connection.")
        }
    }
}

struct AllowlistRow: View {
    let entry: AllowlistEntry
    let onRevoke: () -> Void

    var body: some View {
        HStack(spacing: 16) {
            Image(systemName: "app.badge.checkmark")
                .font(.title2)
                .foregroundColor(.green)
                .frame(width: 40)

            VStack(alignment: .leading, spacing: 4) {
                Text(entry.displayName)
                    .font(.headline)
                Text(entry.appPath)
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .lineLimit(1)
            }

            Spacer()

            Button("Revoke", role: .destructive, action: onRevoke)
                .buttonStyle(.borderless)
        }
        .padding(.vertical, 8)
    }
}
