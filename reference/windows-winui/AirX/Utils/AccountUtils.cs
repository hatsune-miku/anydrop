using AnyDrop.Bridge;
using AnyDrop.Extension;
using AnyDrop.Model;
using AnyDrop.Services;
using AnyDrop.View;
using AnyDrop.ViewModel;
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace AnyDrop.Util
{
    public class AccountUtils
    {
        private static HashSet<string> _blockList = SettingsUtils.ReadBlockList();

        public static void ClearSavedUserInfoAndSignOut()
        {
            SettingsUtils.Delete(Keys.SavedCredential);
            SettingsUtils.Delete(Keys.LoggedInUid);
            GlobalViewModel.Instance.IsSignedIn = false;
            GlobalViewModel.Instance.LoggingInUid = "";
            GlobalViewModel.Instance.LoggingGreetingsName = "AnyDrop User";
        }

        public static bool IsInBlockList(string ipAddress)
        {
            if (!Peer.TryParse(ipAddress, out Peer peer))
            {
                return false;
            }
            return _blockList.Contains(peer.IpAddress);
        }

        public static void AddToBlockList(string ipAddress)
        {
            if (!Peer.TryParse(ipAddress, out Peer peer))
            {
                return;
            }
            _blockList.Add(peer.IpAddress);
            SettingsUtils.WriteBlockList(_blockList);
        }

        public static async Task<bool> SendGreetingsAsync()
        {
            AnyDropCloud.GreetingsResponse greetingsResponse;
            try
            {
                var uid = SettingsUtils.String(Keys.SavedUid, "");
                greetingsResponse = await AnyDropCloud.GreetingsAsync();
                if (greetingsResponse.success)
                {
                    GlobalViewModel.Instance.LoggingGreetingsName = greetingsResponse.name;
                    return true;
                }
            }
            catch (Exception e)
            {
                Debug.WriteLine($"Failed: greetings failed: {e.Message}");
            }

            return false;
        }

        /**
         * Return: true if successfully logged in, otherwise, false.
         */
        public static async Task<bool> TryLoginWithSavedTokenAsync()
        {
            if (!SettingsUtils.Bool(Keys.ShouldAutoSignIn, false))
            {
                return false;
            }

            GlobalViewModel.Instance.IsSignedIn = false;
            Debug.WriteLine("Trying automatic login...");

            // Check credentials!
            if (SettingsUtils.ReadCredentialType() != CredentialType.AnyDropToken)
            {
                Debug.WriteLine("Failed: incorrect credential type");
                return false;
            }

            string token = SettingsUtils.String(Keys.SavedCredential, "");
            if (string.IsNullOrEmpty(token))
            {
                Debug.WriteLine("Failed: empty token");
                return false;
            }

            string uid = SettingsUtils.String(Keys.SavedUid, "");
            if (string.IsNullOrEmpty(uid))
            {
                Debug.WriteLine("Failed: empty uid");
                return false;
            }

            AnyDropCloud.RenewResponse renewResponse;
            try
            {
                renewResponse = await AnyDropCloud.RenewAsync(uid);
                if (renewResponse == null || !renewResponse.success)
                {
                    Debug.WriteLine($"Failed: renew failed.");
                    return false;
                }
            }
            catch (Exception e)
            {
                Debug.WriteLine($"Failed: renew failed: {e.Message}");
                return false;
            }

            await SendGreetingsAsync();

            Debug.WriteLine("Token renewal success.");

            // Update state.
            GlobalViewModel.Instance.LoggingInUid = uid;
            GlobalViewModel.Instance.IsSignedIn = true;

            // Update storage.
            SettingsUtils.Write(Keys.LoggedInUid, uid);
            SettingsUtils.Write(Keys.SavedCredential, renewResponse.token);

            // Initialize websocket.
            WebSocketService.Instance.InitializeAsync().ContinueWith(task =>
            {
                Debug.WriteLine(
                    task.Result
                        ? "Successfully initialized websocket."
                        : "Failed to initialize websocket."
                );
            }, TaskScheduler.Default).FireAndForget();

            return true;
        }

        public static void UserToggleSignInOut()
        {
            if (GlobalViewModel.Instance.IsSignedIn)
            {
                GlobalViewModel.Instance.IsSignedIn = false;
                ClearSavedUserInfoAndSignOut();
                return;
            }

            var window = new LoginWindow();
            window.Activate();
        }
    }
}
