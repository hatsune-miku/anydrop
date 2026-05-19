using AnyDrop.ViewModel;
using CommunityToolkit.Labs.WinUI;
using Microsoft.UI.Xaml.Controls;
using System.Linq;

namespace AnyDrop.Pages
{
    public sealed partial class SelectPeerPage : Page
    {
        private SelectPeerWindowViewModel ViewModel = new();

        public delegate void OnPeerSelectedHandler(Model.PeerItem peer);

        public event OnPeerSelectedHandler OnPeerSelected;

        public SelectPeerPage()
        {
            this.InitializeComponent();

            ViewModel.Peers.AddRange(
                AnyDropBridge.GetPeers()
                .Select(peer => new Model.PeerItem(peer))
            );
        }

        private void OnSettingsCardClicked(object sender, Microsoft.UI.Xaml.RoutedEventArgs e)
        {
            if (!(sender is SettingsCard card))
            {
                return;
            }
            var peers = ViewModel.Peers
                .Where(peer => peer.GetDescription() == card.Description as string)
                .ToList();
            if (peers.Any())
            {
                OnPeerSelected(peers.First());
            }
        }
    }
}
