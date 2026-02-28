using App.ViewModels;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;

namespace App.Pages;

public sealed partial class ConnectionsPage : Page
{
    public ConnectionsViewModel ViewModel { get; } = new ConnectionsViewModel();

    public ConnectionsPage()
    {
        InitializeComponent();
    }

    protected override async void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        await ViewModel.LoadAsync();
    }

    private async void Refresh_Click(object sender, Microsoft.UI.Xaml.RoutedEventArgs e)
        => await ViewModel.LoadAsync();
}
