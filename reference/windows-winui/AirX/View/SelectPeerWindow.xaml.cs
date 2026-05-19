using AnyDrop.Bridge;
using AnyDrop.Model;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Navigation;
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading.Tasks;
using Windows.Foundation;
using Windows.Foundation.Collections;

namespace AnyDrop.View
{
    public sealed partial class SelectPeerWindow : BaseWindow
    {
        public delegate void OnPeerSelectedHandler(PeerItem peer);

        private OnPeerSelectedHandler _handler = null;

        public SelectPeerWindow()
        {
            this.InitializeComponent();

            PrepareWindow(
                new WindowParameters
                {
                    Title = "Select Peers",
                    WidthPortion = 850 / 3840.0 * 1.25,
                    HeightPortion = 1125 / 2160.0 * 1.25,
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

        public void OnPeerSelected(Model.PeerItem peer)
        {
            Close();
            if (_handler != null)
            {
                _handler(peer);
            }
        }

        public void SelectPeer(OnPeerSelectedHandler handler)
        {
            _handler = handler;
            Activate();
        }
    }
}
