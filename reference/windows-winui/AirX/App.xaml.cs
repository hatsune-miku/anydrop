using AnyDrop.Bridge;
using AnyDrop.Util;

namespace AnyDrop
{
    public partial class App : Microsoft.UI.Xaml.Application
    {
        // The logical equivalent of main() or WinMain().
        public App()
        {
            this.InitializeComponent();
        }

        // Invoked when the application is launched.
        protected override void OnLaunched(Microsoft.UI.Xaml.LaunchActivatedEventArgs args)
        {
            SettingsUtils.TryInitializeConfigurationsForFirstRun();

            // Debug console
            if (SettingsUtils.Bool(Keys.ShouldShowConsole, false))
            {
                AnyDropBridge.RedirectAnyDropStdoutToDebugConsole();
            }

            AnyDropNative.anydrop_init();

            var window = new View.TrayIconHolderWindow();
            window.Activate();

            // TODO: Call Deinit when the app is closed.
        }

        public static Microsoft.UI.Xaml.Window LoginWindowShared;

    }
}
