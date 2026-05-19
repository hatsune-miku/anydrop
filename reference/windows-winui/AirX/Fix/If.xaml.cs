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

namespace AnyDrop.Fix
{
    public sealed partial class If : UserControl
    {
        public If()
        {
            this.InitializeComponent();
        }

        public bool Condition
        {
            get { return (bool)GetValue(ShowContentProperty); }
            set { SetValue(ShowContentProperty, value); }
        }

        public static readonly DependencyProperty ShowContentProperty =
            DependencyProperty.Register("Condition", typeof(bool), typeof(If), new PropertyMetadata(false, OnShowContentChanged));

        private static void OnShowContentChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            var control = (If)d;
            control.ContentPresenter.Visibility = (bool)e.NewValue ? Visibility.Visible : Visibility.Collapsed;
        }
    }
}
