import SwiftUI

struct ProvidersView: View {
    @EnvironmentObject var appState: AppState
    @State private var showingAddProvider = false
    @State private var selectedProvider: Provider?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Providers")
                    .font(.largeTitle)
                    .bold()

                Spacer()

                Button(action: { showingAddProvider = true }) {
                    Label("Add Provider", systemImage: "plus")
                }
                .buttonStyle(.borderedProminent)
            }
            .padding()

            Divider()

            if appState.providers.isEmpty {
                VStack(spacing: 12) {
                    Image(systemName: "server.rack")
                        .font(.system(size: 48))
                        .foregroundColor(.secondary)
                    Text("No providers configured")
                        .font(.title3)
                        .foregroundColor(.secondary)
                    Text("Add API key, OAuth, or local providers to get started")
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List {
                    ForEach(appState.providers) { provider in
                        ProviderRow(provider: provider, onEdit: {
                            selectedProvider = provider
                        }, onDelete: {
                            Task {
                                _ = await ServiceClient.shared.removeProvider(providerId: provider.providerId)
                                await appState.refreshData()
                            }
                        })
                    }
                }
                .listStyle(.inset)
            }
        }
        .sheet(isPresented: $showingAddProvider) {
            AddProviderSheet(isPresented: $showingAddProvider)
                .environmentObject(appState)
        }
        .sheet(item: $selectedProvider) { provider in
            EditProviderSheet(provider: provider, isPresented: .init(
                get: { selectedProvider != nil },
                set: { if !$0 { selectedProvider = nil } }
            ))
            .environmentObject(appState)
        }
    }
}

struct ProviderRow: View {
    let provider: Provider
    let onEdit: () -> Void
    let onDelete: () -> Void

    var body: some View {
        HStack(spacing: 16) {
            Image(systemName: providerIcon)
                .font(.title2)
                .foregroundColor(providerColor)
                .frame(width: 40)

            VStack(alignment: .leading, spacing: 4) {
                Text(provider.name)
                    .font(.headline)

                Text(providerTypeText)
                    .font(.caption)
                    .foregroundColor(.secondary)

                if let url = provider.baseUrl {
                    Text(url)
                        .font(.caption)
                        .foregroundColor(.secondary)
                        .lineLimit(1)
                } else if let path = provider.localPath {
                    Text(path)
                        .font(.caption)
                        .foregroundColor(.secondary)
                        .lineLimit(1)
                }
            }

            Spacer()

            HStack(spacing: 8) {
                Button(action: onEdit) {
                    Image(systemName: "pencil")
                }
                .buttonStyle(.plain)

                Button(action: onDelete) {
                    Image(systemName: "trash")
                        .foregroundColor(.red)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.vertical, 8)
    }

    private var providerIcon: String {
        switch provider.type {
        case .apiKey: return "key.fill"
        case .oauth: return "person.badge.key.fill"
        case .local: return "externaldrive.fill"
        }
    }

    private var providerColor: Color {
        switch provider.type {
        case .apiKey: return .blue
        case .oauth: return .green
        case .local: return .orange
        }
    }

    private var providerTypeText: String {
        switch provider.type {
        case .apiKey: return "API Key"
        case .oauth: return "OAuth"
        case .local: return "Local"
        }
    }
}

struct AddProviderSheet: View {
    @EnvironmentObject var appState: AppState
    @Binding var isPresented: Bool
    @State private var providerId = ""
    @State private var name = ""
    @State private var selectedType: ProviderType = .apiKey
    @State private var baseUrl = ""
    @State private var localPath = ""

    var body: some View {
        VStack(spacing: 20) {
            Text("Add Provider")
                .font(.title2)
                .bold()

            Form {
                TextField("Provider ID (e.g., 'openai', 'anthropic')", text: $providerId)

                TextField("Display Name", text: $name)

                Picker("Type", selection: $selectedType) {
                    Text("API Key").tag(ProviderType.apiKey)
                    Text("OAuth").tag(ProviderType.oauth)
                    Text("Local").tag(ProviderType.local)
                }

                if selectedType == .apiKey || selectedType == .oauth {
                    TextField("Base URL", text: $baseUrl)
                        .textContentType(.URL)
                }

                if selectedType == .local {
                    TextField("Local Path", text: $localPath)
                }
            }
            .formStyle(.grouped)

            HStack {
                Button("Cancel") {
                    isPresented = false
                }
                .keyboardShortcut(.cancelAction)

                Spacer()

                Button("Add") {
                    Task {
                        let provider = Provider(
                            providerId: providerId,
                            name: name,
                            type: selectedType,
                            baseUrl: baseUrl.isEmpty ? nil : baseUrl,
                            localPath: localPath.isEmpty ? nil : localPath
                        )
                        _ = await ServiceClient.shared.addProvider(provider)
                        await appState.refreshData()
                        isPresented = false
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(providerId.isEmpty || name.isEmpty)
            }
        }
        .padding()
        .frame(width: 500, height: 450)
    }
}

struct EditProviderSheet: View {
    let provider: Provider
    @Binding var isPresented: Bool
    @EnvironmentObject var appState: AppState
    @State private var name: String
    @State private var baseUrl: String
    @State private var localPath: String

    init(provider: Provider, isPresented: Binding<Bool>) {
        self.provider = provider
        self._isPresented = isPresented
        self._name = State(initialValue: provider.name)
        self._baseUrl = State(initialValue: provider.baseUrl ?? "")
        self._localPath = State(initialValue: provider.localPath ?? "")
    }

    var body: some View {
        VStack(spacing: 20) {
            Text("Edit Provider")
                .font(.title2)
                .bold()

            Form {
                TextField("Provider ID", text: .constant(provider.providerId))
                    .disabled(true)

                TextField("Display Name", text: $name)

                Text("Type: \(providerTypeText)")
                    .foregroundColor(.secondary)

                if provider.type == .apiKey || provider.type == .oauth {
                    TextField("Base URL", text: $baseUrl)
                        .textContentType(.URL)
                }

                if provider.type == .local {
                    TextField("Local Path", text: $localPath)
                }
            }
            .formStyle(.grouped)

            HStack {
                Button("Cancel") {
                    isPresented = false
                }
                .keyboardShortcut(.cancelAction)

                Spacer()

                Button("Save") {
                    Task {
                        let updatedProvider = Provider(
                            providerId: provider.providerId,
                            name: name,
                            type: provider.type,
                            baseUrl: baseUrl.isEmpty ? nil : baseUrl,
                            localPath: localPath.isEmpty ? nil : localPath
                        )
                        _ = await ServiceClient.shared.addProvider(updatedProvider)
                        await appState.refreshData()
                        isPresented = false
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(name.isEmpty)
            }
        }
        .padding()
        .frame(width: 500, height: 450)
    }

    private var providerTypeText: String {
        switch provider.type {
        case .apiKey: return "API Key"
        case .oauth: return "OAuth"
        case .local: return "Local"
        }
    }
}
