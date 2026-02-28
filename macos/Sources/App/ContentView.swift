import SwiftUI

struct ContentView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        NavigationSplitView {
            SidebarView()
        } detail: {
            DetailView()
        }
        .task {
            await appState.refreshData()
        }
    }
}

struct SidebarView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        List(AppState.Tab.allCases, id: \.self, selection: $appState.selectedTab) { tab in
            Label(tab.rawValue, systemImage: tab.icon)
                .tag(tab)
        }
        .navigationTitle("FireBox")
        .frame(minWidth: 200)
    }
}

struct DetailView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        Group {
            switch appState.selectedTab {
            case .dashboard:
                DashboardView()
            case .connections:
                ConnectionsView()
            case .providers:
                ProvidersView()
            case .routes:
                RoutesView()
            case .allowlist:
                AllowlistView()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
