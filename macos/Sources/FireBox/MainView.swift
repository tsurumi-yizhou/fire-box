// SPDX-License-Identifier: Apache-2.0
// Fire Box — macOS Native Layer — Main Window View
//
// NavigationSplitView-based main window layout inspired by SFM.
// Sidebar on the left, detail content on the right.

import SwiftUI

// MARK: - Navigation Page

enum NavigationPage: String, CaseIterable, Identifiable {
    case overview
    case apps
    case providers
    case settings

    var id: String { rawValue }

    var title: String {
        switch self {
        case .overview: "Overview"
        case .apps: "Apps"
        case .providers: "Providers"
        case .settings: "Settings"
        }
    }

    var systemImage: String {
        switch self {
        case .overview: "gauge.with.dots.needle.33percent"
        case .apps: "app.badge.checkmark"
        case .providers: "cloud"
        case .settings: "gear"
        }
    }
}

// MARK: - Main View

struct MainView: View {
    @Bindable var state: FireBoxState
    @State private var selection: NavigationPage = .overview

    var body: some View {
        NavigationSplitView {
            SidebarView(selection: $selection, state: state)
                .navigationSplitViewColumnWidth(180)
        } detail: {
            detailView
                .navigationTitle(selection.title)
                .navigationSplitViewColumnWidth(min: 500, ideal: 620)
        }
        .frame(minHeight: 500)
        .sheet(item: $state.pendingApproval) { approval in
            ApprovalView(state: state, approval: approval)
        }
        .sheet(item: $state.oauthPrompt) { prompt in
            OAuthPromptView(prompt: prompt)
        }
        .toolbar {
            ToolbarItem(placement: .navigation) {
                serviceStatusView
            }
        }
    }

    // MARK: Detail Content

    @ViewBuilder
    private var detailView: some View {
        switch selection {
        case .overview:
            DashboardView(state: state)
        case .apps:
            AppsView(state: state)
        case .providers:
            ProvidersView(state: state)
        case .settings:
            SettingsView(state: state)
        }
    }

    // MARK: Toolbar

    private var serviceStatusView: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(state.isConnected ? .green : .red)
                .frame(width: 8, height: 8)
            Text(state.isConnected ? "Connected" : "Disconnected")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }
}
