using AnyDrop.Model;
using AnyDrop.Util;
using AnyDrop.View;
using AnyDrop.ViewModel;
using CommunityToolkit.Mvvm.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace AnyDrop.Pages
{
    public partial class NewTextPage : Page
    {
        private NewTextViewModel ViewModel = new();
        private NewTextWindow _instance;

        public NewTextPage()
        {
            this.InitializeComponent();
        }

        public void SetWindowInstance(NewTextWindow instance)
        {
            this._instance = instance;
        }

        public void UpdateInformation(string title, Peer peer)
        {
            ViewModel.Title = title;
            ViewModel.Peer = peer;
        }

        [RelayCommand]
        private void Block()
        {
            AccountUtils.AddToBlockList(ViewModel.Peer.IpAddress);
            _instance?.Close();
        }

        [RelayCommand]
        private void Cancel()
        {

        }

        private void OnBlockClicked(object sender, RoutedEventArgs e)
        {
            _ = new ContentDialog()
            {
                Title = "Blocking " + ViewModel.Peer,
                Content = "Are you sure to block " + ViewModel.Peer + "?",
                PrimaryButtonText = "Block",
                SecondaryButtonText = "Cancel",
                PrimaryButtonCommand = BlockCommand,
                SecondaryButtonCommand = CancelCommand,
                XamlRoot = Content.XamlRoot,
            }.ShowAsync();
        }
    }
}
