import SwiftUI

struct RoutesView: View {
    @EnvironmentObject var appState: AppState
    @State private var showingAddRule = false
    @State private var selectedRule: RouteRule?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Routes")
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
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text(rule.virtualModelId)
                        .font(.headline)

                    HStack(spacing: 8) {
                        Text(rule.displayName)
                            .font(.caption)
                            .foregroundColor(.secondary)

                        Text("•")
                            .foregroundColor(.secondary)

                        Text(rule.strategy)
                            .font(.caption)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(Color.accentColor.opacity(0.15))
                            .cornerRadius(4)
                    }
                }

                Spacer()

                Text("\(rule.targets.count) target(s)")
                    .font(.caption)
                    .foregroundColor(.secondary)

                Button(action: onEdit) {
                    Image(systemName: "pencil")
                }
                .buttonStyle(.borderless)
            }

            ForEach(Array(rule.targets.enumerated()), id: \.offset) { _, target in
                HStack(spacing: 4) {
                    Image(systemName: "arrow.right")
                        .font(.caption2)
                        .foregroundColor(.secondary)
                    Text("\(target.providerId) / \(target.modelId)")
                        .font(.system(.caption, design: .monospaced))
                        .foregroundColor(.secondary)
                }
                .padding(.leading, 24)
            }
        }
        .padding(.vertical, 8)
    }
}

// MARK: - Add Route

struct AddRouteRuleSheet: View {
    @Binding var isPresented: Bool
    @EnvironmentObject var appState: AppState

    @State private var virtualModelId = ""
    @State private var displayName = ""
    @State private var strategy = "failover"
    @State private var providerId = ""
    @State private var modelId = ""

    private let strategies = ["failover", "round_robin", "lowest_latency"]

    var body: some View {
        VStack(spacing: 16) {
            Text("Add Route Rule")
                .font(.title2)
                .bold()

            Form {
                TextField("Virtual Model ID", text: $virtualModelId)
                TextField("Display Name", text: $displayName)
                Picker("Strategy", selection: $strategy) {
                    ForEach(strategies, id: \.self) { s in
                        Text(s).tag(s)
                    }
                }

                Section("Target") {
                    TextField("Provider ID", text: $providerId)
                    TextField("Model ID", text: $modelId)
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
                    let rule = RouteRule(
                        virtualModelId: virtualModelId,
                        displayName: displayName.isEmpty ? virtualModelId : displayName,
                        strategy: strategy,
                        targets: [RouteTarget(providerId: providerId, modelId: modelId)]
                    )
                    Task {
                        _ = await ServiceClient.shared.addRouteRule(rule)
                        await appState.refreshData()
                        isPresented = false
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(virtualModelId.isEmpty || providerId.isEmpty || modelId.isEmpty)
            }
        }
        .padding()
        .frame(width: 500, height: 450)
    }
}

// MARK: - Edit Route

struct EditRouteRuleSheet: View {
    let rule: RouteRule
    @Binding var isPresented: Bool
    @EnvironmentObject var appState: AppState

    var body: some View {
        VStack(spacing: 16) {
            Text("Route: \(rule.virtualModelId)")
                .font(.title2)
                .bold()

            VStack(alignment: .leading, spacing: 12) {
                LabeledContent("Display Name", value: rule.displayName)
                LabeledContent("Strategy", value: rule.strategy)

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
                        _ = await ServiceClient.shared.removeRouteRule(virtualModelId: rule.virtualModelId)
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
