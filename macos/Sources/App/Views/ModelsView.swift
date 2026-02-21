import SwiftUI

struct ModelsView: View {
    @EnvironmentObject var appState: AppState
    @State private var showingAddRule = false
    @State private var selectedRule: RouteRule?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Model Routes")
                    .font(.largeTitle)
                    .bold()

                Spacer()

                Button(action: { showingAddRule = true }) {
                    Label("Add Route", systemImage: "plus")
                }
                .buttonStyle(.borderedProminent)
            }
            .padding()

            Divider()

            if appState.routeRules.isEmpty {
                VStack(spacing: 12) {
                    Image(systemName: "arrow.triangle.branch")
                        .font(.system(size: 48))
                        .foregroundColor(.secondary)
                    Text("No route rules configured")
                        .font(.title3)
                        .foregroundColor(.secondary)
                    Text("Add rules to route model requests to specific providers")
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List {
                    ForEach(appState.routeRules) { rule in
                        RouteRuleRow(rule: rule, onEdit: {
                            selectedRule = rule
                        })
                    }
                }
                .listStyle(.inset)
            }
        }
        .sheet(isPresented: $showingAddRule) {
            AddRouteRuleSheet(isPresented: $showingAddRule)
                .environmentObject(appState)
        }
        .sheet(item: $selectedRule) { rule in
            EditRouteRuleSheet(rule: rule, isPresented: .init(
                get: { selectedRule != nil },
                set: { if !$0 { selectedRule = nil } }
            ))
            .environmentObject(appState)
        }
    }
}

struct RouteRuleRow: View {
    let rule: RouteRule
    let onEdit: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text(rule.alias)
                        .font(.headline)

                    Text("\(rule.targets.count) target(s)")
                        .font(.caption)
                        .foregroundColor(.secondary)
                }

                Spacer()

                Button(action: onEdit) {
                    Image(systemName: "pencil")
                }
                .buttonStyle(.plain)
            }

            VStack(alignment: .leading, spacing: 8) {
                ForEach(Array(rule.targets.enumerated()), id: \.offset) { index, target in
                    HStack(spacing: 8) {
                        Text("\(index + 1).")
                            .font(.caption)
                            .foregroundColor(.secondary)
                            .frame(width: 20, alignment: .trailing)

                        Label {
                            Text("\(target.providerId) / \(target.modelId)")
                                .font(.system(.body, design: .monospaced))
                        } icon: {
                            Image(systemName: "arrow.right")
                                .foregroundColor(.blue)
                        }
                    }
                }
            }
            .padding(.leading)
        }
        .padding(.vertical, 8)
    }
}

struct AddRouteRuleSheet: View {
    @EnvironmentObject var appState: AppState
    @Binding var isPresented: Bool
    @State private var alias = ""
    @State private var selectedProvider = ""
    @State private var selectedModel = ""

    var body: some View {
        VStack(spacing: 20) {
            Text("Add Route Rule")
                .font(.title2)
                .bold()

            Form {
                TextField("Alias (e.g., 'default', 'fast-model')", text: $alias)

                Picker("Provider", selection: $selectedProvider) {
                    Text("Select Provider").tag("")
                    ForEach(appState.providers) { provider in
                        Text(provider.name).tag(provider.providerId)
                    }
                }

                Picker("Model", selection: $selectedModel) {
                    Text("Select Model").tag("")
                    ForEach(appState.models.filter { $0.providerId == selectedProvider || selectedProvider.isEmpty }) { model in
                        Text(model.modelId).tag(model.modelId)
                    }
                }
                .disabled(selectedProvider.isEmpty)
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
                        let rule = RouteRule(
                            alias: alias,
                            targets: [RouteTarget(providerId: selectedProvider, modelId: selectedModel)]
                        )
                        _ = await ServiceClient.shared.addRouteRule(rule)
                        await appState.refreshData()
                        isPresented = false
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(alias.isEmpty || selectedProvider.isEmpty || selectedModel.isEmpty)
            }
        }
        .padding()
        .frame(width: 500, height: 400)
    }
}

struct EditRouteRuleSheet: View {
    let rule: RouteRule
    @Binding var isPresented: Bool
    @EnvironmentObject var appState: AppState

    var body: some View {
        VStack(spacing: 20) {
            Text("Edit Route Rule")
                .font(.title2)
                .bold()

            VStack(alignment: .leading, spacing: 12) {
                Text("Alias: \(rule.alias)")
                    .font(.headline)

                Text("Targets:")
                    .font(.subheadline)
                    .foregroundColor(.secondary)

                ForEach(Array(rule.targets.enumerated()), id: \.offset) { index, target in
                    Text("\(index + 1). \(target.providerId) / \(target.modelId)")
                        .font(.system(.body, design: .monospaced))
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding()
            .background(Color(nsColor: .controlBackgroundColor))
            .cornerRadius(8)

            HStack {
                Button("Delete", role: .destructive) {
                    Task {
                        _ = await ServiceClient.shared.removeRouteRule(alias: rule.alias)
                        await appState.refreshData()
                        isPresented = false
                    }
                }

                Spacer()

                Button("Close") {
                    isPresented = false
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
        .frame(width: 500, height: 400)
    }
}
