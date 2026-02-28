using System;
using System.Threading.Tasks;
using App.Services;
using App.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;

namespace App.Pages;

public sealed partial class ProvidersPage : Page
{
    public ProvidersViewModel ViewModel { get; } = new ProvidersViewModel();

    public ProvidersPage()
    {
        InitializeComponent();
    }

    protected override async void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        await ViewModel.LoadAsync();
    }

    // -----------------------------------------------------------------------
    // Add API Key
    // -----------------------------------------------------------------------

    private async void AddApiKey_Click(object sender, RoutedEventArgs e)
    {
        var nameBox      = new TextBox { 
            PlaceholderText = ResourceHelper.GetString("AddApiKeyDialogNamePlaceholder") };
        var typeBox      = new ComboBox { 
            ItemsSource = new[] { "openai", "anthropic", "dashscope", "gemini" }, 
            SelectedIndex = 0 };
        var keyBox       = new PasswordBox { 
            PlaceholderText = ResourceHelper.GetString("AddApiKeyDialogKeyPlaceholder") };
        var urlBox       = new TextBox { 
            PlaceholderText = ResourceHelper.GetString("AddApiKeyDialogUrlPlaceholder") };

        var panel = new StackPanel { Spacing = 8 };
        panel.Children.Add(new TextBlock { 
            Text = ResourceHelper.GetString("AddApiKeyDialogNameLabel/Text") });
        panel.Children.Add(nameBox);
        panel.Children.Add(new TextBlock { 
            Text = ResourceHelper.GetString("AddApiKeyDialogTypeLabel/Text") });
        panel.Children.Add(typeBox);
        panel.Children.Add(new TextBlock { 
            Text = ResourceHelper.GetString("AddApiKeyDialogKeyLabel/Text") });
        panel.Children.Add(keyBox);
        panel.Children.Add(new TextBlock { 
            Text = ResourceHelper.GetString("AddApiKeyDialogUrlLabel/Text") });
        panel.Children.Add(urlBox);

        var dialog = new ContentDialog
        {
            Title = ResourceHelper.GetString("AddApiKeyDialogTitle"),
            Content = new ScrollViewer { Content = panel, MaxHeight = 400 },
            PrimaryButtonText = ResourceHelper.GetString("AddApiKeyDialogPrimaryButton"),
            CloseButtonText = ResourceHelper.GetString("AddApiKeyDialogCloseButton"),
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = XamlRoot
        };

        if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;

        var baseUrl = string.IsNullOrWhiteSpace(urlBox.Text) ? null : urlBox.Text.Trim();
        await ViewModel.AddApiKeyProviderAsync(
            nameBox.Text.Trim(),
            typeBox.SelectedItem?.ToString() ?? "openai",
            keyBox.Password,
            baseUrl);
    }

    // -----------------------------------------------------------------------
    // Add OAuth (device-code flow, 2 steps)
    // -----------------------------------------------------------------------

    private async void AddOAuth_Click(object sender, RoutedEventArgs e)
    {
        var nameBox = new TextBox { 
            PlaceholderText = ResourceHelper.GetString("AddApiKeyDialogNamePlaceholder") };
        var typeBox = new ComboBox { 
            ItemsSource = new[] { "copilot" }, 
            SelectedIndex = 0 };

        var step1Panel = new StackPanel { Spacing = 8 };
        step1Panel.Children.Add(new TextBlock { 
            Text = ResourceHelper.GetString("AddApiKeyDialogNameLabel/Text") });
        step1Panel.Children.Add(nameBox);
        step1Panel.Children.Add(new TextBlock { 
            Text = ResourceHelper.GetString("AddApiKeyDialogTypeLabel/Text") });
        step1Panel.Children.Add(typeBox);

        var step1 = new ContentDialog
        {
            Title = ResourceHelper.GetString("AddOAuthDialogTitle"),
            Content = step1Panel,
            PrimaryButtonText = ResourceHelper.GetString("AddOAuthDialogPrimaryButton"),
            CloseButtonText = ResourceHelper.GetString("AddApiKeyDialogCloseButton"),
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = XamlRoot
        };

        if (await step1.ShowAsync() != ContentDialogResult.Primary) return;

        // Start OAuth → get challenge
        Services.OAuthChallengeDto challenge;
        try
        {
            challenge = await ViewModel.StartOAuthAsync(
                nameBox.Text.Trim(),
                typeBox.SelectedItem?.ToString() ?? "copilot");
        }
        catch (Exception ex)
        {
            await ShowErrorAsync(ex.Message);
            return;
        }

        // Show device code + wait for user to authorise
        var codeBlock = new TextBlock
        {
            Text = challenge.UserCode,
            Style = (Microsoft.UI.Xaml.Style)Application.Current.Resources["TitleLargeTextBlockStyle"],
            IsTextSelectionEnabled = true
        };
        var hint = new TextBlock
        {
            Text = ResourceHelper.GetString(
                "AddOAuthDialogHintFormat", 
                challenge.VerificationUri, 
                challenge.ExpiresIn / 60),
            TextWrapping = Microsoft.UI.Xaml.TextWrapping.Wrap
        };

        var step2Panel = new StackPanel { Spacing = 12 };
        step2Panel.Children.Add(hint);
        step2Panel.Children.Add(codeBlock);

        var step2 = new ContentDialog
        {
            Title = ResourceHelper.GetString("AddOAuthDialogStep2Title"),
            Content = step2Panel,
            PrimaryButtonText = ResourceHelper.GetString("AddOAuthDialogStep2PrimaryButton"),
            CloseButtonText = ResourceHelper.GetString("AddApiKeyDialogCloseButton"),
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = XamlRoot
        };

        if (await step2.ShowAsync() != ContentDialogResult.Primary)
        {
            ViewModel.CancelOAuth();
            return;
        }

        try
        {
            await ViewModel.CompleteOAuthAsync();
        }
        catch (Exception ex)
        {
            await ShowErrorAsync(ex.Message);
        }
    }

    // -----------------------------------------------------------------------
    // Add Local (llama.cpp / GGUF)
    // -----------------------------------------------------------------------

    private async void AddLocal_Click(object sender, RoutedEventArgs e)
    {
        var nameBox = new TextBox { 
            PlaceholderText = ResourceHelper.GetString("AddApiKeyDialogNamePlaceholder") };
        var pathBox = new TextBox { 
            PlaceholderText = ResourceHelper.GetString("AddLocalDialogPathPlaceholder") };

        var panel = new StackPanel { Spacing = 8 };
        panel.Children.Add(new TextBlock { 
            Text = ResourceHelper.GetString("AddApiKeyDialogNameLabel/Text") });
        panel.Children.Add(nameBox);
        panel.Children.Add(new TextBlock { 
            Text = ResourceHelper.GetString("AddLocalDialogPathLabel/Text") });
        panel.Children.Add(pathBox);

        var dialog = new ContentDialog
        {
            Title = ResourceHelper.GetString("AddLocalDialogTitle"),
            Content = panel,
            PrimaryButtonText = ResourceHelper.GetString("AddLocalDialogPrimaryButton"),
            CloseButtonText = ResourceHelper.GetString("AddApiKeyDialogCloseButton"),
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = XamlRoot
        };

        if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;

        await ViewModel.AddLocalProviderAsync(nameBox.Text.Trim(), pathBox.Text.Trim());
    }

    // -----------------------------------------------------------------------
    // Delete
    // -----------------------------------------------------------------------

    private async void Delete_Click(object sender, RoutedEventArgs e)
    {
        var id = (string)((FrameworkElement)sender).Tag;
        var dialog = new ContentDialog
        {
            Title = ResourceHelper.GetString("DeleteProviderDialogTitle"),
            Content = ResourceHelper.GetString("DeleteProviderDialogContent"),
            PrimaryButtonText = ResourceHelper.GetString("DeleteProviderDialogPrimaryButton"),
            CloseButtonText = ResourceHelper.GetString("DeleteProviderDialogCloseButton"),
            DefaultButton = ContentDialogButton.Close,
            XamlRoot = XamlRoot
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
            await ViewModel.DeleteProviderAsync(id);
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    private async Task ShowErrorAsync(string message)
    {
        var d = new ContentDialog
        {
            Title = ResourceHelper.GetString("ErrorDialogTitle"),
            Content = message,
            CloseButtonText = ResourceHelper.GetString("ErrorDialogCloseButton"),
            XamlRoot = XamlRoot
        };
        await d.ShowAsync();
    }
}
