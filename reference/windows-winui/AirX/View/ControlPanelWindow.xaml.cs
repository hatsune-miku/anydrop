// Copyright (c) Microsoft Corporation and Contributors.
// Licensed under the MIT License.

using System.Collections.Generic;

namespace AnyDrop.View
{
    public partial class ControlPanelWindow : BaseWindow
    {
        protected LoginWindowViewModel ViewModel;

        public delegate void WindowSizeChangedHandler(Microsoft.UI.Xaml.WindowSizeChangedEventArgs args);
        protected HashSet<WindowSizeChangedHandler> Subscribers = new();

        public static ControlPanelWindow Instance { get; private set; }

        public ControlPanelWindow()
        {
            this.InitializeComponent();
            this.ViewModel = new LoginWindowViewModel();
            Instance = this;

            PrepareWindow(
                new WindowParameters
                {
                    Title = "Control Panel",
                    WidthPortion = 1810 / 3840.0 * 1.25,
                    HeightPortion = 1230 / 2160.0 * 1.25,
                    CenterScreen = true,
                    TopMost = false,
                    Resizable = true,
                    HaveMaximumButton = false,
                    HaveMinimumButton = true,
                    EnableMicaEffect = true,
                }
            );
            SetTitleBar(titleBar);
        }

        public void SubscribeToSizeChange(WindowSizeChangedHandler handler)
        {
            Subscribers.Add(handler);
        }

        public void UnsubscribeToSizeChange(WindowSizeChangedHandler handler)
        {
            Subscribers.Remove(handler);
        }

        private void ControlPanelWindow_SizeChanged(object sender, Microsoft.UI.Xaml.WindowSizeChangedEventArgs args)
        {
            foreach (var handler in Subscribers)
            {
                handler(args);
            }
        }
    }
}
