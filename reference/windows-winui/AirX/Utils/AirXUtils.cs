using AnyDrop.Extension;
using AnyDrop.Model;
using AnyDrop.View;
using AnyDrop.ViewModel;
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Windows.Win32;
using Windows.Win32.Foundation;

namespace AnyDrop.Util
{
    class AnyDropUtils
    {
        public static async Task UserSendFileAsync()
        {
            var files = await FileUtils.OpenFileDialogAsync();
            if (files.Count == 0 || files.First() == null)
            {
                return;
            }

            if (AnyDropBridge.GetPeers().Count == 0)
            {
                PInvoke.MessageBox(
                    HWND.Null, "No peers available", "Error", Windows.Win32.UI.WindowsAndMessaging.MESSAGEBOX_STYLE.MB_ICONINFORMATION);
                return;
            }

            var window = new SelectPeerWindow();
            window.SelectPeer(peer =>
            {
                foreach (var file in files)
                {
                    AnyDropBridge.TrySendFile(file.Path, peer.Value);
                }
            });
        }

        public static void UserToggleService()
        {
            if (GlobalViewModel.Instance.IsServiceOnline)
            {
                AnyDropBridge.TryStopAnyDropService();
            }
            else
            {
                AnyDropBridge.TryStartAnyDropService();
            }
        }
    }
}
