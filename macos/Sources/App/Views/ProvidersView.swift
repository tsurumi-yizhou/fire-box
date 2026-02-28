import SwiftUI

struct ProvidersView: View {
    @EnvironmentObject var appState: AppState
    @State private var showingAddSheet = false
    @State private var confirmDelete: Provider?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Providers")
                    .font(.largeTitle)
                    .bold()

                Spacer()

                Button(action: { showingAddSheet = true }) {
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
                        ProviderRow(provider: provider, onDelete: {
                            confirmDelete = provider
                        })
                    }
                }
                .listStyle(.inset)
            }
        }
        .sheet(isPresented: $showingAddSheet) {
            AddProviderSheet(isPresented: $showingAddSheet)
                .environmentObject(appState)
        }
        .alert("Delete Provider", isPresented: .init(
            get: { confirmDelete != nil },
            set: { if !$0 { confirmDelete = nil } }
        )) {
            Button("Cancel", role: .cancel) { confirmDelete = nil }
            Button("Delete", role: .destructive) {
                if let p = confirmDelete {
                    Task {
                        _ = await ServiceClient.shared.removeProvider(providerId: p.providerId)
                        await appState.refreshData()
                    }
                }
                confirmDelete = nil
            }
        } message: {
            Text("Remove \"\(confirmDelete?.displayName ?? "")\"? This cannot be undone.")
        }
    }
}

struct ProviderRow: View {
    let provider: Provider
    let onDelete: () -> Void

    var body: some View {
        HStack(spacing: 16) {
            Image(systemName: providerIcon)
                .font(.title2)
                .foregroundColor(providerColor)
                .frame(width: 40)

            VStack(alignment: .leading, spacing: 4) {
                Text(provider.displayName)
                    .font(.headline)
                Text(provider.providerType)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }

            Spacer()

            Text(provider.providerId)
                .font(.caption)
                .foregroundColor(.secondary)

            Button(role: .destructive, action: onDelete) {
                Image(systemName: "trash")
            }
            .buttonStyle(.borderless)
        }
        .padding(.vertical, 8)
    }

    private var providerIcon: String {
        switch provider.providerType {
        case "openai": return "brain"
        case "anthropic": return "brain.head.profile"
        case "copilot": return "person.badge.key"
        case "dashscope": return "cloud"
        case "llamacpp": return "desktopcomputer"
        case "ollama": return "server.rack"
        default: return "questionmark.circle"
        }
    }

    private var providerColor: Color {
        switch provider.providerType {
        case "openai": return .green
        case "anthropic": return .orange
        case "copilot": return .blue
        case "dashscope": return .purple
        case "llamacpp": return .gray
        case "ollama": return .teal
        default: return .secondary
        }
    }
}

// MARK: - Add Provider

enum AddProviderMode: String, CaseIterable {
    case apiKey = "API Key"
    case oauth = "OAuth"
    case local = "Local"
}

struct AddProviderSheet: View {
    @Binding var isPresented: Bool
    @EnvironmentObject var appState: AppState
    @State private var mode: AddProviderMode = .apiKey

    var body: some View {
        VStack(spacing: 0) {
            Text("Add Provider")
                .font(.title2)
                .bold()
                .padding()

            Picker("Type", selection: $mode) {
                ForEach(AddProviderMode.allCases, id: \.self) { m in
                    Text(m.rawValue).tag(m)
                }
            }
            .pickerStyle(.segmented)
            .padding(.horizontal)

            Divider()
                .padding(.vertical, 8)

            switch mode {
            case .apiKey:
                AddApiKeyProviderForm(isPresented: $isPresented)
                    .environmentObject(appState)
            case .oauth:
                AddOAuthProviderForm(isPresented: $isPresented)
                    .environmentObject(appState)
            case .local:
                AddLocalProviderForm(isPresented: $isPresented)
                    .environmentObject(appState)
            }
        }
        .frame(width: 500, height: 420)
    }
}

// MARK: - API Key Flow

struct AddApiKeyProviderForm: View {
    @Binding var isPresented: Bool
    @EnvironmentObject var appState: AppState

    @State private var name = ""
    @State private var providerType = "openai"
    @State private var apiKey = ""
    @State private var baseUrl = ""

    private let providerTypes = ["openai", "anthropic", "ollama"]

    var body: some View {
        Form {
            TextField("Name", text: $name)
            Picker("Provider", selection: $providerType) {
                ForEach(providerTypes, id: \.self) { Text($0) }
            }
            SecureField("API Key", text: $apiKey)
            TextField("Base URL (optional)", text: $baseUrl)
        }
        .formStyle(.grouped)
        .padding(.horizontal)

        Spacer()

        HStack {
            Button("Cancel") { isPresented = false }
                .keyboardShortcut(.cancelAction)
            Spacer()
            Button("Add") {
                Task {
                    _ = await ServiceClient.shared.addApiKeyProvider(
                        name: name,
                        providerType: providerType,
                        apiKey: apiKey,
                        baseUrl: baseUrl.isEmpty ? nil : baseUrl
                    )
                    await appState.refreshData()
                    isPresented = false
                }
            }
            .keyboardShortcut(.defaultAction)
            .disabled(name.isEmpty || apiKey.isEmpty)
        }
        .padding()
    }
}

// MARK: - OAuth Flow

struct AddOAuthProviderForm: View {
    @Binding var isPresented: Bool
    @EnvironmentObject var appState: AppState

    @State private var name = ""
    @State private var providerType = "copilot"
    @State private var challenge: OAuthChallenge?
    @State private var polling = false
    @State private var errorMessage: String?

    private let providerTypes = ["copilot", "dashscope"]

    var body: some View {
        VStack(spacing: 16) {
            if let challenge = challenge {
                // Step 2: Show device code
                VStack(spacing: 12) {
                    Text("Enter this code:")
                        .font(.headline)
                    Text(challenge.userCode)
                        .font(.system(.largeTitle, design: .monospaced))
                        .textSelection(.enabled)
                    Text("at")
                        .foregroundColor(.secondary)
                    Link(challenge.verificationUri, destination: URL(string: challenge.verificationUri)!)
                        .font(.callout)

                    if polling {
                        ProgressView("Waiting for authorization…")
                            .padding(.top)
                    }
                }
                .padding()
            } else {
                // Step 1: Pick provider
                Form {
                    TextField("Name", text: $name)
                    Picker("Provider", selection: $providerType) {
                        ForEach(providerTypes, id: \.self) { Text($0) }
                    }
                }
                .formStyle(.grouped)
                .padding(.horizontal)
            }

            if let err = errorMessage {
                Text(err)
                    .foregroundColor(.red)
                    .font(.caption)
            }

            Spacer()

            HStack {
                Button("Cancel") { isPresented = false }
                    .keyboardShortcut(.cancelAction)
                Spacer()
                if challenge == nil {
                    Button("Start") {
                        Task { await startOAuth() }
                    }
                    .keyboardShortcut(.defaultAction)
                    .disabled(name.isEmpty)
                }
            }
            .padding()
        }
    }

    private func startOAuth() async {
        errorMessage = nil
        guard let ch = await ServiceClient.shared.addOAuthProvider(name: name, providerType: providerType) else {
            errorMessage = "Failed to start OAuth flow."
            return
        }
        challenge = ch
        polling = true

        // Open verification URL in browser.
        if let url = URL(string: ch.verificationUri) {
            NSWorkspace.shared.open(url)
        }

        // Poll for completion.
        let interval = max(ch.interval, 5)
        for _ in 0..<(ch.expiresIn / interval) {
            try? await Task.sleep(for: .seconds(interval))
            let ok = await ServiceClient.shared.completeOAuth(providerType: providerType, deviceCode: ch.deviceCode)
            if ok {
                await appState.refreshData()
                isPresented = false
                return
            }
        }
        polling = false
        errorMessage = "Authorization timed out."
    }
}

// MARK: - Local Flow

struct AddLocalProviderForm: View {
    @Binding var isPresented: Bool
    @EnvironmentObject var appState: AppState

    @State private var name = ""
    @State private var modelPath = ""

    var body: some View {
        Form {
            TextField("Name", text: $name)
            HStack {
                TextField("Model Path (.gguf)", text: $modelPath)
                Button("Browse…") {
                    let panel = NSOpenPanel()
                    panel.allowedContentTypes = [.data]
                    panel.allowsMultipleSelection = false
                    if panel.runModal() == .OK, let url = panel.url {
                        modelPath = url.path
                    }
                }
            }
        }
        .formStyle(.grouped)
        .padding(.horizontal)

        Spacer()

        HStack {
            Button("Cancel") { isPresented = false }
                .keyboardShortcut(.cancelAction)
            Spacer()
            Button("Add") {
                Task {
                    _ = await ServiceClient.shared.addLocalProvider(name: name, modelPath: modelPath)
                    await appState.refreshData()
                    isPresented = false
                }
            }
            .keyboardShortcut(.defaultAction)
            .disabled(name.isEmpty || modelPath.isEmpty)
        }
        .padding()
    }
}
