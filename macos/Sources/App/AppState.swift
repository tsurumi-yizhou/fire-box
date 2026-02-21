import Foundation
import Combine

@MainActor
class AppState: ObservableObject {
    @Published var selectedTab: Tab = .dashboard
    @Published var metrics: MetricsSnapshot?
    @Published var connections: [Connection] = []
    @Published var providers: [Provider] = []
    @Published var models: [Model] = []
    @Published var routeRules: [RouteRule] = []

    private let serviceClient = ServiceClient.shared
    private var cancellables = Set<AnyCancellable>()

    enum Tab: String, CaseIterable {
        case dashboard = "Dashboard"
        case connections = "Connections"
        case models = "Models"
        case providers = "Providers"

        var icon: String {
            switch self {
            case .dashboard: return "chart.bar.fill"
            case .connections: return "network"
            case .models: return "cpu"
            case .providers: return "server.rack"
            }
        }
    }

    init() {
        startPeriodicUpdates()
    }

    private func startPeriodicUpdates() {
        Timer.publish(every: 2.0, on: .main, in: .common)
            .autoconnect()
            .sink { [weak self] _ in
                Task { @MainActor [weak self] in
                    await self?.refreshData()
                }
            }
            .store(in: &cancellables)
    }

    func refreshData() async {
        async let metricsTask = serviceClient.getMetricsSnapshot()
        async let connectionsTask = serviceClient.listConnections()
        async let providersTask = serviceClient.listProviders()
        async let modelsTask = serviceClient.listModels()
        async let routesTask = serviceClient.listRouteRules()

        self.metrics = await metricsTask
        self.connections = await connectionsTask
        self.providers = await providersTask
        self.models = await modelsTask
        self.routeRules = await routesTask
    }
}
