import Combine
import Foundation

@MainActor
class AppState: ObservableObject {
    @Published var selectedTab: Tab = .dashboard
    @Published var serviceRunning: Bool = false
    @Published var metrics: MetricsSnapshot?
    @Published var connections: [Connection] = []
    @Published var providers: [Provider] = []
    @Published var routeRules: [RouteRule] = []
    @Published var allowlist: [AllowlistEntry] = []

    private let serviceClient = ServiceClient.shared
    private var cancellables = Set<AnyCancellable>()

    enum Tab: String, CaseIterable {
        case dashboard = "Dashboard"
        case connections = "Connections"
        case providers = "Providers"
        case routes = "Routes"
        case allowlist = "Allowlist"

        var icon: String {
            switch self {
            case .dashboard: return "chart.bar.fill"
            case .connections: return "network"
            case .providers: return "server.rack"
            case .routes: return "arrow.triangle.branch"
            case .allowlist: return "checkmark.shield"
            }
        }
    }

    init() {
        startPeriodicUpdates()
    }

    private func startPeriodicUpdates() {
        Timer.publish(every: 5.0, on: .main, in: .common)
            .autoconnect()
            .sink { [weak self] _ in
                Task { @MainActor [weak self] in
                    await self?.refreshData()
                }
            }
            .store(in: &cancellables)
    }

    func refreshData() async {
        async let statusTask = serviceClient.checkServiceStatus()
        async let metricsTask = serviceClient.getMetricsSnapshot()
        async let connectionsTask = serviceClient.listConnections()
        async let providersTask = serviceClient.listProviders()
        async let routesTask = serviceClient.listRouteRules()
        async let allowlistTask = serviceClient.getAllowlist()

        self.serviceRunning = await statusTask
        self.metrics = await metricsTask
        self.connections = await connectionsTask
        self.providers = await providersTask
        self.routeRules = await routesTask
        self.allowlist = await allowlistTask
    }
}
