using AnyDrop.Model;
using CommunityToolkit.Mvvm.ComponentModel;
using Microsoft.UI.Xaml.Media;
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace AnyDrop.ViewModel
{
    public partial class NewFileViewModel : ObservableObject
    {
        public NewFileViewModel(ReceiveFile receiveFile)
        {
            receivingFile = receiveFile;
        }

        [ObservableProperty]
        Brush backgroundColor = GetSolidColorBrush("#FFD9D9D9");

        [ObservableProperty]
        Brush accentColor = GetSolidColorBrush("#FF6F9DAB");

        [ObservableProperty]
        Brush dimmedAccentColor = GetSolidColorBrush("#FF1B4E5E");

        [ObservableProperty]
        ReceiveFile receivingFile;

        private static SolidColorBrush GetSolidColorBrush(string hex)
        {
            hex = hex.Replace("#", string.Empty);
            byte a = (byte)(Convert.ToUInt32(hex.Substring(0, 2), 16));
            byte r = (byte)(Convert.ToUInt32(hex.Substring(2, 2), 16));
            byte g = (byte)(Convert.ToUInt32(hex.Substring(4, 2), 16));
            byte b = (byte)(Convert.ToUInt32(hex.Substring(6, 2), 16));
            return new SolidColorBrush(Windows.UI.Color.FromArgb(a, r, g, b));
        }
    }
}
