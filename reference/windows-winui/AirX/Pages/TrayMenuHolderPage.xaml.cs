using AnyDrop.Extension;
using AnyDrop.Util;
using AnyDrop.Utils;
using AnyDrop.View;
using AnyDrop.ViewModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using System.Diagnostics;
using System.Threading.Tasks;

namespace AnyDrop.Pages
{
    public sealed partial class TrayMenuHolderPage : Page
    {
        private AboutWindow aboutWindow = null;
        private GlobalViewModel ViewModel = GlobalViewModel.Instance;

        public TrayMenuHolderPage()
        {
            this.InitializeComponent();
        }

        [RelayCommand]
        public void ExitApplication()
        {
            Process.GetCurrentProcess().Kill();
        }

        [RelayCommand]
        public void ShowAboutAnyDrop()
        {
            if (aboutWindow != null)
            {
                aboutWindow.Close();
            }
            aboutWindow = new AboutWindow();
            aboutWindow.Activate();
        }

        [RelayCommand]
        public void ToggleService()
        {
            AnyDropUtils.UserToggleService();
        }

        [RelayCommand]
        public void OpenControlPanel()
        {
            var window = new ControlPanelWindow();
            window.Activate();
        }

        [RelayCommand]
        public void ToggleSignInOut()
        {
            AccountUtils.UserToggleSignInOut();
        }

        [RelayCommand]
        public void SendFile()
        {
            AnyDropUtils.UserSendFileAsync()
                .FireAndForget();
        }
    }
}
