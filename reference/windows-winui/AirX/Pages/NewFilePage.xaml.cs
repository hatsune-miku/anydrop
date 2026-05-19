using AnyDrop.Extension;
using AnyDrop.Util;
using AnyDrop.View;
using AnyDrop.ViewModel;
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

namespace AnyDrop.Pages
{
    public sealed partial class NewFilePage : Page
    {
        private NewFileViewModel ViewModel;
        private GlobalViewModel GlobalViewModel = GlobalViewModel.Instance;

        private NewFileWindow _instance;

        public int FileId { get; set; } = 0;

        public NewFilePage()
        {
            this.InitializeComponent();
        }

        private void OnPageLoading(FrameworkElement sender, object args)
        {
            ViewModel = GlobalViewModel.Instance.ReceiveFiles[FileId];
            DataContext = ViewModel;
        }

        private void OnPageLoaded(object sender, RoutedEventArgs e)
        {
        }

        public void SetWindowInstance(NewFileWindow instance)
        {
            this._instance = instance;
        }

        public string GetFileSizeDescription()
        {
            ulong sizeInBytes;
            try
            {
                sizeInBytes = ViewModel.ReceivingFile.TotalSize;
                return FileUtils.GetFileSizeDescription(sizeInBytes);
            }
            catch (Exception)
            {
                return "<error>";
            }
        }

        public string GetFrom()
        {
            try
            {
                return "File from " + ViewModel.ReceivingFile.From.ToString();
            }
            catch (Exception)
            {
                return "<error>";
            }
        }

        private async Task HandleStopAsync()
        {
            var result = await UIUtils.ShowContentDialogYesNoAsync(
                "Stop", "Are you sure to stop receiving this file?", "Stop", "Don't Stop", Content.XamlRoot);

            if (result == ContentDialogResult.Primary)
            {
                ViewModel.ReceivingFile.Status = AnyDropBridge.FileStatus.CancelledByReceiver;
                _instance?.Close();
            }
        }

        private void OnStopOrOpenFolderClicked(object sender, RoutedEventArgs e)
        {
            if (ViewModel.ReceivingFile.Status == AnyDropBridge.FileStatus.Completed)
            {
                FileUtils.OpenFolderInExplorer(ViewModel.ReceivingFile.LocalSaveFullPath);
                _instance?.Close();
            }
            else
            {
                HandleStopAsync().FireAndForget();
            }
        }


        private void OnBlockClicked(object sender, RoutedEventArgs e)
        {
            var target = ViewModel.ReceivingFile.From.ToString();
            new ContentDialog()
            {
                Title = "Blocking " + target,
                Content = "Are you sure you want to block (" + target + ") and stop receiving everything from them?",
                PrimaryButtonText = "Block",
                SecondaryButtonText = "Cancel",
                XamlRoot = Content.XamlRoot,
            }.ShowAsync().AsTask().ContinueWith(t =>
            {
                try
                {
                    if (t.Result == ContentDialogResult.Primary)
                    {
                        AccountUtils.AddToBlockList(target);
                    }
                }
                catch (Exception) { }
            }, TaskScheduler.Default).FireAndForget();
        }
    }
}
