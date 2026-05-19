using Microsoft.UI;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using System;
using System.Threading.Tasks;
using WinRT.Interop;
using Windows.Foundation;
using Windows.Graphics;
using AnyDrop.View;
using Microsoft.UI.Xaml.Controls;
using AnyDrop.Extension;
using Windows.Win32;
using Windows.Win32.Foundation;

namespace AnyDrop.Util
{
    public static class UIUtils
    {
        public static AppWindow GetAppWindow(Window window)
        {
            var hWnd = WindowNative.GetWindowHandle(window);
            var wndId = Win32Interop.GetWindowIdFromWindow(hWnd);
            return AppWindow.GetFromWindowId(wndId);
        }

        public static void MoveWindowToCenterScreen(AppWindow window)
        {
            var point = CalculateCenterScreenPoint(
                window.Size.Width, window.Size.Height);
            window.Move(new PointInt32((int) point.X, (int) point.Y));
        }

        public static Size GetPrimaryScreenSize()
        {
            int screenWidth = PInvoke.GetSystemMetrics(Windows.Win32.UI.WindowsAndMessaging.SYSTEM_METRICS_INDEX.SM_CXSCREEN);
            int screenHeight = PInvoke.GetSystemMetrics(Windows.Win32.UI.WindowsAndMessaging.SYSTEM_METRICS_INDEX.SM_CYSCREEN);
            return new Size(screenWidth, screenHeight);
        }

        public static void ShowContentDialog(string title, string content, XamlRoot xamlRoot)
        {
            new ContentDialog()
            {
                Title = title,
                Content = content,
                CloseButtonText = "OK",
                XamlRoot = xamlRoot
            }.ShowAsync()
            .AsTask()
            .FireAndForget();
        }

        public static async Task<ContentDialogResult> ShowContentDialogYesNoAsync(string title, string content, string primaryButtonText, string secondaryButtonText, XamlRoot xamlRoot)
        {
            return await new ContentDialog()
            {
                Title = title,
                Content = content,
                PrimaryButtonText = primaryButtonText,
                SecondaryButtonText = secondaryButtonText,
                XamlRoot = xamlRoot
            }.ShowAsync().AsTask();
        }

        public static void SetWindowVisibility(Window window, bool visible)
        {
            nint rawHandle = WindowNative.GetWindowHandle(window);
            PInvoke.ShowWindow(
                new HWND(rawHandle),
                visible
                    ? Windows.Win32.UI.WindowsAndMessaging.SHOW_WINDOW_CMD.SW_SHOW
                    : Windows.Win32.UI.WindowsAndMessaging.SHOW_WINDOW_CMD.SW_HIDE
            );
        }

        public static Point CalculateCenterScreenPoint(int width, int height)
        {
            var size = GetPrimaryScreenSize();
            return new Point
            {
                X = size.Width / 2 - width / 2,
                Y = size.Height / 2 - height / 2
            };
        }
    }
}
