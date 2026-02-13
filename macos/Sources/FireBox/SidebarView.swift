// SPDX-License-Identifier: Apache-2.0
// Fire Box — macOS Native Layer — Sidebar View
//
// List-based sidebar navigation for the main window.

import SwiftUI

struct SidebarView: View {
    @Binding var selection: NavigationPage
    @Bindable var state: FireBoxState

    var body: some View {
        List(selection: $selection) {
            Section("Dashboard") {
                Label("Overview", systemImage: NavigationPage.overview.systemImage)
                    .tag(NavigationPage.overview)
            }

            Section("Management") {
                Label("Apps", systemImage: NavigationPage.apps.systemImage)
                    .tag(NavigationPage.apps)
                Label("Providers", systemImage: NavigationPage.providers.systemImage)
                    .tag(NavigationPage.providers)
            }

            Section {
                Label("Settings", systemImage: NavigationPage.settings.systemImage)
                    .tag(NavigationPage.settings)
            }
        }
        .listStyle(.sidebar)
    }
}
