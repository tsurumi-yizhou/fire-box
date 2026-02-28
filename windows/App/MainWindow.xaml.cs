using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace App;

public sealed partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(AppTitleBar);

        // Navigate to Dashboard by default
        NavView.SelectedItem = NavView.MenuItems[0];
    }

    private void NavView_SelectionChanged(NavigationView sender,
        NavigationViewSelectionChangedEventArgs args)
    {
        var tag = (args.SelectedItem as NavigationViewItem)?.Tag?.ToString();
        var pageType = tag switch
        {
            "dashboard"   => typeof(Pages.DashboardPage),
            "connections" => typeof(Pages.ConnectionsPage),
            "providers"   => typeof(Pages.ProvidersPage),
            "routes"      => typeof(Pages.RoutesPage),
            "allowlist"   => typeof(Pages.AllowlistPage),
            _             => (Type?)null
        };
        if (pageType is not null)
            ContentFrame.Navigate(pageType);
    }
}
