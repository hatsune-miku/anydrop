using CommunityToolkit.Mvvm.ComponentModel;
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Windows.Storage;

namespace AnyDrop.Util
{
    public enum Keys
    {
        IsNotFirstRun,

        DiscoveryServiceClientPort,
        DiscoveryServiceServerPort,
        DataServiceListenPort,
        DataServiceAddressIpV4,
        GroupIdentifier,
        AutoDiscovery,

        LoggedInUid,
        SavedUid,
        SavedCredentialType,
        SavedCredential,
        ShouldAutoSignIn,

        BlockList,

        ShouldShowConsole,
        ShouldShowAdvancedSettings,

        IsKafkaProducer,
        IsKafkaConsumer,
        AnyDropCloudAddress,

        SaveFilePath,

        NewTextPopupDisplayTimeMillis,
    }

    public enum CredentialType
    {
        Password,
        AnyDropToken,
        GoogleToken
    }

    public class SettingsUtils
    {
        private static ApplicationDataContainer localSettings =
            ApplicationData.Current.LocalSettings;

        public static void TryInitializeConfigurationsForFirstRun()
        {
            if (Bool(Keys.IsNotFirstRun, false))
            {
                return;
            }
            
            Write(Keys.DiscoveryServiceClientPort, 0);
            Write(Keys.DiscoveryServiceServerPort, 9818);
            Write(Keys.DataServiceListenPort, 9819);
            Write(Keys.DataServiceAddressIpV4, "0.0.0.0");
            Write(Keys.GroupIdentifier, 0);
            Write(Keys.AutoDiscovery, true);
            Write(Keys.ShouldAutoSignIn, false);
            Write(Keys.ShouldShowConsole, false);
            Write(Keys.ShouldShowAdvancedSettings, false);
            Write(Keys.IsKafkaProducer, true);
            Write(Keys.IsKafkaConsumer, true);
            Write(Keys.AnyDropCloudAddress, "https://anydrop.eggtartc.com");
            Write(Keys.SaveFilePath, "C:\\AnyDropFiles");
            Write(Keys.NewTextPopupDisplayTimeMillis, 6000);

            Write(Keys.IsNotFirstRun, true);
        }

        public static string String(Keys key, string def)
        {
            return localSettings.Values[key.ToString()] as string ?? def;
        }

        public static bool Bool(Keys key, bool def)
        {
            if (bool.TryParse(String(key, "!"), out bool ret))
            {
                return ret;
            }
            return def;
        }

        public static int Int(Keys key, int def)
        {
            if (int.TryParse(String(key, "!"), out int ret))
            {
                return ret;
            }
            return def;
        }

        public static double Double(Keys key, double def)
        {
            if (double.TryParse(String(key, "!"), out double ret))
            {
                return ret;
            }
            return def;
        }

        public static void Delete(Keys key)
        {
            localSettings.Values.Remove(key.ToString());
        }

        // Utility methods
        public static string SavedCredential()
        {
            return String(Keys.SavedCredential, "");
        }

        public static CredentialType ReadCredentialType()
        {
            string rawValue = String(Keys.SavedCredentialType, CredentialType.Password.ToString());
            return Enum.TryParse<CredentialType>(rawValue, out CredentialType credentialType)
                ? credentialType
                : CredentialType.Password;
        }

        public static bool ShouldEnableAutoDiscovery()
        {
            return Bool(Keys.AutoDiscovery, true);
        }

        public static void SetAutoDiscovery(bool enabled)
        {
            Write(Keys.AutoDiscovery, enabled);
        }

        public static HashSet<string> ReadBlockList()
        {
            string rawValue = String(Keys.BlockList, "");
            return rawValue.Split(
                ",",
                StringSplitOptions.TrimEntries 
                    & StringSplitOptions.RemoveEmptyEntries
            ).ToHashSet();
        }

        public static void WriteBlockList(HashSet<string> blockList)
        {
            Write(Keys.BlockList, string.Join(',', blockList));
        }

        public static void Write(Keys key, object value)
        {
            localSettings.Values[key.ToString()] = value.ToString();
        }
    }
}
