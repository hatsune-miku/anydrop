using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using AnyDrop.Utils;
using CommunityToolkit.Mvvm.ComponentModel;

namespace AnyDrop
{
    public partial class LoginWindowViewModel: ObservableObject
    {
        [ObservableProperty]
        public string buttonTitle = "ConfirmSignIn".Text();

        [ObservableProperty]
        public string uid = "";

        [ObservableProperty]
        public string password = "";

        [ObservableProperty]
        public bool shouldRememberPassword = true;

        [ObservableProperty]
        public bool isLoggingIn = false;
    }
}
