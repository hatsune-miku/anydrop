using AnyDrop.Extension;
using AnyDrop.Helper;
using AnyDrop.Services;
using AnyDrop.Util;
using AnyDrop.Utils;
using AnyDrop.View;
using AnyDrop.ViewModel;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Documents;
using Microsoft.UI.Xaml.Input;
using System;
using System.Diagnostics;
using System.Drawing;
using System.Threading.Tasks;

namespace AnyDrop.Pages
{
    public sealed partial class LoginPage : Page
    {
        public LoginWindowViewModel ViewModel { get; set; }
        private GoogleSignInHelper googleSignInHelper = new();

        public LoginPage()
        {
            InitializeComponent();

            ViewModel = new LoginWindowViewModel();
            textBoxUid.Focus(FocusState.Programmatic);
        }

        private async Task HandleLoginAsync()
        {
            if (ViewModel.IsLoggingIn)
            {
                return;
            }

            if (ViewModel.Uid.Trim() == "" || ViewModel.Password.Trim() == "")
            {
                return;
            }

            ViewModel.IsLoggingIn = true;
            AnyDropCloud.LoginResponse response;
            try
            {
                response = await AnyDropCloud.LoginAsync(ViewModel.Uid, ViewModel.Password);
                if (!response.success)
                {
                    UIUtils.ShowContentDialog(
                        "Error", "Login failed: " + response.message,
                        Content.XamlRoot
                    );
                    return;
                }
            }
            catch (AnyDropCloud.UnauthorizedException)
            {
                UIUtils.ShowContentDialog(
                    "Error", "Login failed: unknown reason.",
                    Content.XamlRoot
                );
                return;
            }
            finally
            {
                ViewModel.IsLoggingIn = false;
            }

            // Success
            SettingsUtils.Write(Keys.SavedUid, ViewModel.Uid);
            SettingsUtils.Write(Keys.LoggedInUid, ViewModel.Uid);
            SettingsUtils.Write(Keys.SavedCredential, response.token);
            SettingsUtils.Write(Keys.SavedCredentialType, CredentialType.AnyDropToken);
            GlobalViewModel.Instance.LoggingInUid = ViewModel.Uid;
            GlobalViewModel.Instance.IsSignedIn = true;

            SettingsUtils.Write(
                Keys.ShouldAutoSignIn,
                ViewModel.ShouldRememberPassword
            );

            await AccountUtils.SendGreetingsAsync();

            // Initialize WebSocket
            WebSocketService.Instance.InitializeAsync().FireAndForget();

            LoginWindow.Instance?.Close();
        }

        private void OnLoginButtonClicked(object sender, RoutedEventArgs e)
        {
            HandleLoginAsync().FireAndForget();
        }

        private void onTextBoxesKeyDown(object sender, KeyRoutedEventArgs e)
        {
            if (e.Key != Windows.System.VirtualKey.Enter)
            {
                return;
            }
            OnLoginButtonClicked(sender, null);
        }

        private void GoogleLoginButton_Click(object sender, RoutedEventArgs e)
        {
            googleSignInHelper.TrySignInAsync().FireAndForget();
        }
    }
}
