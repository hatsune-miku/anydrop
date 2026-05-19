using AnyDrop.Util;
using System.Security.Policy;
using System.Threading.Channels;
using WinRT.Interop;

namespace AnyDrop.View
{
    public sealed partial class NewFileWindow : BaseWindow
    {
        private const int WINDOW_WIDTH = 640;
        private const int WINDOW_HEIGHT = 340;

        public NewFileWindow(int fileId)
        {
            this.InitializeComponent();

            newFilePage.SetWindowInstance(this);
            newFilePage.FileId = fileId;

            PrepareWindow(
                new WindowParameters
                {
                    Title = "New File Window",
                    WidthPortion = WINDOW_WIDTH / 3840.0 * 1.25,
                    HeightPortion = WINDOW_HEIGHT / 2160.0 * 1.25,
                    CenterScreen = false,
                    TopMost = true,
                    Resizable = false,
                    HaveMaximumButton = false,
                    HaveMinimumButton = false,
                    EnableMicaEffect = false,
                }
            );
            ExtendsContentIntoTitleBar = true;

            var screenSize = UIUtils.GetPrimaryScreenSize();
            AppWindow.Move(new Windows.Graphics.PointInt32(
                (int)screenSize.Width - AppWindow.Size.Width - 64,
                (int)screenSize.Height - AppWindow.Size.Height - 92
            ));
        }

        private void OnClosed(object sender, Microsoft.UI.Xaml.WindowEventArgs args)
        {

        }
    }
}
