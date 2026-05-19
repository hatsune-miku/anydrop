using AnyDrop.Model;
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
using Windows.Foundation;
using Windows.Foundation.Collections;

namespace AnyDrop.Pages
{
    public sealed partial class ReceivedFilesPage : Page
    {
        private ReceiveFilesPageViewModel ViewModel = new();

        public ReceivedFilesPage()
        {
            this.InitializeComponent();
        }

        private void OnPageLoading(FrameworkElement sender, object args)
        {
            GlobalViewModel.Instance.ReceiveFiles.MapChanged += OnMapChanged;
            RefreshView();
        }

        private void OnMapChanged(IObservableMap<int, NewFileViewModel> sender, IMapChangedEventArgs<int> @event)
        {
            RefreshView();
        }

        private void RefreshView()
        {
            ViewModel.ReceiveFiles = GlobalViewModel.Instance.ReceiveFiles.Values
                .Select(item => item.ReceivingFile)
                .SkipWhile(item => item == ReceiveFile.Sample)
                .ToList();
            ViewModel.NoReceiveFiles = ViewModel.ReceiveFiles.Count == 0;
        }
    }
}
