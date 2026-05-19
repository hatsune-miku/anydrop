using AnyDrop.Util;
using AnyDrop.ViewModel;
using CommunityToolkit.Mvvm.ComponentModel;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace AnyDrop.Model
{
    public enum SettingsItemType
    {
        Boolean,
        String,
    }

    public partial class SettingsItem : ObservableObject
    {
        public delegate bool ValidatorFunction(string valueStringRepresentation);

        [ObservableProperty]
        string title;

        [ObservableProperty]
        string description;

        [ObservableProperty]
        bool isAdvanced;

        [ObservableProperty]
        Keys settingsKey;

        [ObservableProperty]
        ValidatorFunction validator;

        [ObservableProperty]
        SettingsItemType itemType;

        public ConfigurationViewModel ViewModel { get; set; }
        public XamlRoot XamlRoot { get; set; }

        string stringRepresentation = null;

        public string ReadAsString()
        {
            if (stringRepresentation == null)
            {
                stringRepresentation = SettingsUtils.String(SettingsKey, "");
            }
            return stringRepresentation;
        }

        public bool ReadAsBoolean()
        {
            if (stringRepresentation == null)
            {
                stringRepresentation = SettingsUtils.String(SettingsKey, "false")
                    .ToLower();
            }
            if (bool.TryParse(stringRepresentation, out bool result))
            {
                return result;
            }
            return false;
        }

        public void SetAsString(string newValue)
        {
            stringRepresentation = newValue;
        }

        public void OnTextChanged(object sender, TextChangedEventArgs e)
        {
            ViewModel.IsUnsaved = true;
        }

        public void SetAsBoolean(bool newValue)
        {
            stringRepresentation = newValue.ToString();
            SettingsUtils.Write(SettingsKey, stringRepresentation.ToLower());
        }

        public void OnButtonValueSaved(object sender, RoutedEventArgs e)
        {
            if (Validator != null && !Validator(stringRepresentation))
            {
                if (XamlRoot != null)
                {
                    UIUtils.ShowContentDialog("Error", "You have entered an invalid value.", XamlRoot);
                }
                return;
            }
            ViewModel.IsUnsaved = false;
            SettingsUtils.Write(SettingsKey, stringRepresentation);
        }
    }
}
