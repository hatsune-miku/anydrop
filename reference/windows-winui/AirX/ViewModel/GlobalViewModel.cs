using AnyDrop.Data;
using AnyDrop.Model;
using CommunityToolkit.Mvvm.ComponentModel;
using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Drawing;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace AnyDrop.ViewModel
{
    public partial class GlobalViewModel : ObservableObject
    {
        public static GlobalViewModel Instance { get; set; }
            = new GlobalViewModel();

        [ObservableProperty]
        public bool isServiceOnline = false;

        [ObservableProperty]
        public bool isDiscoveryServiceOnline = false;

        [ObservableProperty]
        public bool isTextServiceOnline = false;

        [ObservableProperty]
        public bool isSignedIn = false;

        [ObservableProperty]
        public bool isApplicationExiting = false;

        [ObservableProperty]
        public string loggingInUid = "";

        [ObservableProperty]
        public string loggingGreetingsName = "AnyDrop User";

        // Key: file id
        [ObservableProperty]
        public ObservableDictionary<int, NewFileViewModel> receiveFiles = new()
        {
            { -1, new(ReceiveFile.Sample) }
        };

        // Key: file id
        [ObservableProperty]
        public ObservableDictionary<int, SendFile> sendFiles = new();
    }
}
