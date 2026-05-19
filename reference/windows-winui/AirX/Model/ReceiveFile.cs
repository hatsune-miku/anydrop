using AnyDrop.Bridge;
using AnyDrop.ViewModel;
using CommunityToolkit.Mvvm.ComponentModel;
using Microsoft.UI.Xaml;
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace AnyDrop.Model
{
    public partial class ReceiveFile : ObservableObject
    {
        public string RemoteFullPath { get; set; }
        public FileStream WritingStream { get; set; }
        public String LocalSaveFullPath { get; set; }
        public ulong TotalSize { get; set; }
        public int FileId { get; set; }
        public Peer From { get; set; }
        public ulong Progress { get; set; }
        public AnyDropBridge.FileStatus Status { get; set; }

        [ObservableProperty]
        public ulong displayProgress;

        [ObservableProperty]
        public AnyDropBridge.FileStatus displayStatus;

        public static readonly ReceiveFile Sample = new()
        {
            RemoteFullPath = "sample.pdf",
            WritingStream = null,
            LocalSaveFullPath = "",
            Progress = (ulong) (100 * 1024 * 0.65),
            TotalSize = 100 * 1024,
            FileId = -1,
            Status = AnyDropBridge.FileStatus.InProgress,
            From = Peer.Sample,
            DisplayProgress = (ulong)(100 * 1024 * 0.65),
            DisplayStatus = AnyDropBridge.FileStatus.InProgress,
        };

        public void OnCancelAndDelete(object sender, RoutedEventArgs e)
        {
            Status = AnyDropBridge.FileStatus.CancelledByReceiver;
            GlobalViewModel.Instance.ReceiveFiles.TriggerNotifyChanged();
        }
    }
}
