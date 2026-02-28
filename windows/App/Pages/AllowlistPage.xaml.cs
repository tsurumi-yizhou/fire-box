using System;
using System.Threading.Tasks;
using App.Services;
using App.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;

namespace App.Pages;

public sealed partial class AllowlistPage : Page
{
    public AllowlistViewModel ViewModel { get; } = new AllowlistViewModel();

    public AllowlistPage()
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

    private async void Revoke_Click(object sender, RoutedEventArgs e)
    {
        var appPath = (string)((FrameworkElement)sender).Tag;
        var dialog = new ContentDialog
        {
            Title = ResourceHelper.GetString("RevokeAccessDialogTitle"),
            Content = ResourceHelper.GetString("RevokeAccessDialogContentFormat", appPath),
            PrimaryButtonText = ResourceHelper.GetString("RevokeAccessDialogPrimaryButton"),
            CloseButtonText = ResourceHelper.GetString("RevokeAccessDialogCloseButton"),
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = XamlRoot
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
            await ViewModel.RevokeAsync(appPath);
    }
}
