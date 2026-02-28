using System;
using System.Threading.Tasks;
using App.Services;
using App.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;

namespace App.Pages;

public sealed partial class RoutesPage : Page
{
    public RoutesViewModel ViewModel { get; } = new RoutesViewModel();

    public RoutesPage()
    {
        InitializeComponent();
    }

    protected override async void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        await ViewModel.LoadAsync();
    }

    private async void Refresh_Click(object sender, RoutedEventArgs e)
        => await ViewModel.LoadAsync();

    // -----------------------------------------------------------------------
    // Add route dialog
    // -----------------------------------------------------------------------

    private async void AddRoute_Click(object sender, RoutedEventArgs e)
    {
        var vmIdBox      = new TextBox { 
            PlaceholderText = ResourceHelper.GetString("AddRouteDialogVirtualModelIdPlaceholder") };
        var nameBox      = new TextBox { 
            PlaceholderText = ResourceHelper.GetString("AddRouteDialogDisplayNamePlaceholder") };
        var strategyBox  = new ComboBox
        {
            ItemsSource   = new[] { "failover", "round_robin", "lowest_latency" },
            SelectedIndex = 0
        };
        // Single target for simplicity; users can add more via subsequent edits
        var providerBox = new TextBox {
            PlaceholderText = ResourceHelper.GetString("AddRouteDialogTargetProviderPlaceholder") };
        var modelBox    = new TextBox { 
            PlaceholderText = ResourceHelper.GetString("AddRouteDialogTargetModelPlaceholder") };

        var panel = new StackPanel { Spacing = 8 };
        panel.Children.Add(new TextBlock { 
            Text = ResourceHelper.GetString("AddRouteDialogVirtualModelIdLabel/Text") });
        panel.Children.Add(vmIdBox);
        panel.Children.Add(new TextBlock { 
            Text = ResourceHelper.GetString("AddRouteDialogDisplayNameLabel/Text") });
        panel.Children.Add(nameBox);
        panel.Children.Add(new TextBlock { 
            Text = ResourceHelper.GetString("AddRouteDialogStrategyLabel/Text") });
        panel.Children.Add(strategyBox);
        panel.Children.Add(new TextBlock { 
            Text = ResourceHelper.GetString("AddRouteDialogTargetProviderLabel/Text") });
        panel.Children.Add(providerBox);
        panel.Children.Add(new TextBlock { 
            Text = ResourceHelper.GetString("AddRouteDialogTargetModelLabel/Text") });
        panel.Children.Add(modelBox);

        var dialog = new ContentDialog
        {
            Title = ResourceHelper.GetString("AddRouteDialogTitle"),
            Content = new ScrollViewer { Content = panel, MaxHeight = 480 },
            PrimaryButtonText = ResourceHelper.GetString("AddRouteDialogPrimaryButton"),
            CloseButtonText = ResourceHelper.GetString("AddRouteDialogCloseButton"),
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = XamlRoot
        };

        if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;

        var targets = new[]
        {
            new RouteTargetDto(providerBox.Text.Trim(), modelBox.Text.Trim())
        };
        var caps = new RouteCapabilitiesDto(); // defaults: chat + streaming

        await ViewModel.SaveRouteAsync(
            vmIdBox.Text.Trim(),
            nameBox.Text.Trim(),
            strategyBox.SelectedItem?.ToString() ?? "failover",
            targets,
            caps);
    }

    // -----------------------------------------------------------------------
    // Delete
    // -----------------------------------------------------------------------

    private async void Delete_Click(object sender, RoutedEventArgs e)
    {
        var virtualModelId = (string)((FrameworkElement)sender).Tag;
        var dialog = new ContentDialog
        {
            Title = ResourceHelper.GetString("DeleteRouteDialogTitle"),
            Content = ResourceHelper.GetString("DeleteRouteDialogContentFormat", virtualModelId),
            PrimaryButtonText = ResourceHelper.GetString("DeleteRouteDialogPrimaryButton"),
            CloseButtonText = ResourceHelper.GetString("DeleteRouteDialogCloseButton"),
            DefaultButton = ContentDialogButton.Close,
            XamlRoot = XamlRoot
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
            await ViewModel.DeleteRouteAsync(virtualModelId);
    }
}
