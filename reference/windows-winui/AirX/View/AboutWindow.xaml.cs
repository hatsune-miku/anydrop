using AnyDrop.Bridge;
using AnyDrop.Utils;
using AnyDrop.ViewModel;

namespace AnyDrop.View
{
    public sealed partial class AboutWindow : BaseWindow
    {
        private readonly AboutViewModel ViewModel;

        public AboutWindow()
        {
            this.InitializeComponent();

            ViewModel = new()
            {
                AnyDropVersion = AnyDropNative.anydrop_version().ToString(),
                AnyDropVersionString = AnyDropBridge.GetVersionString(),
                BuildValue = "2",
                VersionValue = "1.1",
                Copyright = "© 2023 Chang Guan",
            };

            PrepareWindow(
                new WindowParameters
                {
                    Title = "AboutAnyDrop".Text(),
                    WidthPortion = 800 / 3840.0 * 1.25,
                    HeightPortion = 705 / 2160.0 * 1.25,
                    CenterScreen = true,
                    TopMost = true,
                    Resizable = false,
                    HaveMaximumButton = false,
                    HaveMinimumButton = false,
                    EnableMicaEffect = true,
                }
            );
            SetTitleBar(titleBar);
        }
    }
}
