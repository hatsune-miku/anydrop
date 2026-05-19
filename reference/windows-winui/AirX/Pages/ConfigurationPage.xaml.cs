using System.Collections.Generic;
using System.Linq;
using AnyDrop.Util;
using AnyDrop.ViewModel;
using AnyDrop.Model;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using AnyDrop.Extension;
using AnyDrop.Utils;
using AnyDrop.View;
using System;

namespace AnyDrop.Pages
{
    public class SettingsDataTemplateSelector : DataTemplateSelector
    {
        public DataTemplate BooleanSettingsTemplate { get; set; }
        public DataTemplate StringSettingsTemplate { get; set; }

        protected override DataTemplate SelectTemplateCore(object item, DependencyObject container)
        {
            if (item is SettingsItem settingsItem)
            {
                switch (settingsItem.ItemType)
                {
                    case SettingsItemType.String:
                        return StringSettingsTemplate;

                    case SettingsItemType.Boolean:
                        return BooleanSettingsTemplate;

                    default:
                        return StringSettingsTemplate;
                }
            }
            return base.SelectTemplateCore(item, container);
        }
    }

    public sealed partial class ConfigurationPage : Page
    {
        List<SettingsItem> SettingsItems = new();
        ConfigurationViewModel ViewModel = new();

        public ConfigurationPage()
        {
            this.InitializeComponent();
        }

        private void OnPageLoaded(object sender, RoutedEventArgs e)
        {
            SettingsItems.Add(new Model.SettingsItem
            {
                Title = "LANGroupIdentifier.Text".Tr(),
                Description = "LANGroupIdentifier.Description".Tr(),
                SettingsKey = Keys.GroupIdentifier,
                Validator = IsGroupIdentityValid,
                ItemType = SettingsItemType.String,
                IsAdvanced = false,
            });

            SettingsItems.Add(new Model.SettingsItem
            {
                Title = "TextPopupDisplayTime.Text".Tr(),
                Description = "TextPopupDisplayTime.Description".Tr(),
                SettingsKey = Keys.NewTextPopupDisplayTimeMillis,
                ItemType = SettingsItemType.String,
                Validator = IsMillisTimeValid,
                IsAdvanced = false,
            });

            SettingsItems.Add(new Model.SettingsItem
            {
                Title = "EnhancedDiscovery.Text".Tr(),
                Description = "EnhancedDiscovery.Description".Tr(),
                SettingsKey = Keys.AutoDiscovery,
                ItemType = SettingsItemType.Boolean,
                IsAdvanced = false,
            });

            SettingsItems.Add(new Model.SettingsItem
            {
                Title = "ShowDeveloperConsole.Text".Tr(),
                Description = "ShowDeveloperConsole.Description".Tr(),
                SettingsKey = Keys.ShouldShowConsole,
                ItemType = SettingsItemType.Boolean,
                IsAdvanced = true,
            });

            SettingsItems.Add(new Model.SettingsItem
            {
                Title = "ShareClipboardOverInternet.Text".Tr(),
                Description = "ShareClipboardOverInternet.Description".Tr(),
                SettingsKey = Keys.IsKafkaProducer,
                ItemType = SettingsItemType.Boolean,
                IsAdvanced = true,
            });

            SettingsItems.Add(new Model.SettingsItem
            {
                Title = "ReceiveClipboardFromInternet.Text".Tr(),
                Description = "ReceiveClipboardFromInternet.Description".Tr(),
                SettingsKey = Keys.IsKafkaConsumer,
                ItemType = SettingsItemType.Boolean,
                IsAdvanced = true,
            });

            SettingsItems.Add(new Model.SettingsItem
            {
                Title = "LANDiscoveryServerPort.Text".Tr(),
                Description = "LANDiscoveryServerPort.Description".Tr(),
                SettingsKey = Keys.DiscoveryServiceServerPort,
                Validator = IsPortValid,
                ItemType = SettingsItemType.String,
                IsAdvanced = true,
            });

            SettingsItems.Add(new Model.SettingsItem
            {
                Title = "LANDiscoveryClientPort.Text".Tr(),
                Description = "LANDiscoveryClientPort.Description".Tr(),
                SettingsKey = Keys.DiscoveryServiceClientPort,
                Validator = IsPortValid,
                ItemType = SettingsItemType.String,
                IsAdvanced = true,
            });

            SettingsItems.Add(new Model.SettingsItem
            {
                Title = "LANDataServiceListenAddress.Text".Tr(),
                SettingsKey = Keys.DataServiceAddressIpV4,
                Validator = IsIpV4AddressValid,
                ItemType = SettingsItemType.String,
                IsAdvanced = true,
            });

            SettingsItems.Add(new Model.SettingsItem
            {
                Title = "LANDataServiceListenPort.Text".Tr(),
                Description = "LANDataServiceListenPort.Description".Tr(),
                SettingsKey = Keys.DataServiceListenPort,
                Validator = IsPortValid,
                ItemType = SettingsItemType.String,
                IsAdvanced = true,
            });

            SettingsItems.Add(new Model.SettingsItem
            {
                Title = "AnyDropCloudServer.Text".Tr(),
                Description = "AnyDropCloudServer.Description".Tr(),
                SettingsKey = Keys.AnyDropCloudAddress,
                ItemType = SettingsItemType.String,
                IsAdvanced = true,
            });

            foreach (var item in SettingsItems)
            {
                item.XamlRoot = Content.XamlRoot;
                item.ViewModel = ViewModel;
            }

            ApplyLatestItemFilters();
            ControlPanelWindow.Instance?.SubscribeToSizeChange(OnControlPanelWindowSizeChanged);
        }

        private void OnPageUnloaded(object sender, RoutedEventArgs e)
        {
            ControlPanelWindow.Instance?.UnsubscribeToSizeChange(OnControlPanelWindowSizeChanged);
        }

        private void OnControlPanelWindowSizeChanged(Microsoft.UI.Xaml.WindowSizeChangedEventArgs args)
        {
            ScrollView.Height = Math.Max(220, args.Size.Height - 220);
        }

        public void ApplyLatestItemFilters()
        {
            ViewModel.SettingsItems = SettingsItems.Where(
                item => ViewModel.ShouldShowAdvancedSettings || !item.IsAdvanced
            ).ToList();
        }

        public void SetShouldShowAdvancedSettings(bool value)
        {
            ViewModel.ShouldShowAdvancedSettings = value;
            SettingsUtils.Write(Keys.ShouldShowAdvancedSettings, value.ToString().ToLower());
            ApplyLatestItemFilters();
        }

        public bool GetShouldShowAdvancedSettings()
        {
            return ViewModel.ShouldShowAdvancedSettings;
        }

        public string GetTitle()
        {
            return ViewModel.IsUnsaved
                ? "Preferences".Text() + " - " + "Edited".Text()
                : "Perferences".Text();
        }

        private bool IsPortValid(string portRepr)
        {
            return int.TryParse(portRepr, out int port)
                && port == 0 || (1024 < port && port < 65535);
        }

        private bool IsIpV4AddressValid(string address)
        {
            var parts = address.Split('.');
            return (parts.Length == 4 
                && parts.All(p => int.TryParse(p, out int res) && res >= 0 && res <= 255));
        }

        private bool IsGroupIdentityValid(string groupIdentityRepr)
        {
            return int.TryParse(groupIdentityRepr, out int groupIdentity)
                && 0 <= groupIdentity && groupIdentity <= 255;
        }

        private bool IsMillisTimeValid(string millisRepr)
        {
            return int.TryParse(millisRepr, out int res)
                && 1000 <= res && res <= 10000;
        }

        private void OnCleanCacheClicked(object sender, RoutedEventArgs args)
        {
            UIUtils.ShowContentDialogYesNoAsync(
                "ClearCache".Text(), "ClearCacheWarning".Text(),
                "ConfirmClear".Text(), "Cancel".Text(), Content.XamlRoot)
                .FireAndForget();
        }
    }
}
