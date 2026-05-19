using System;
using System.Collections.Generic;
using System.Linq;
using System.Resources;
using System.Text;
using System.Threading.Tasks;
using Windows.ApplicationModel.Resources;

namespace AnyDrop.Utils
{
    internal class I18nUtils
    {
        private static readonly ResourceLoader _loader = ResourceLoader.GetForViewIndependentUse("Resources");

        public static string Tr(string uid)
        {
            uid = uid.Replace(".", "/");
            return _loader.GetString(uid);
        }
    }

    public static class StringI18nExtension
    {
        public static string Tr(this string uid)
        {
            return I18nUtils.Tr(uid);
        }

        public static string Text(this string uid)
        {
            string text = I18nUtils.Tr(uid + "/Text");
            if (text.Length > 0) return text;

            string content = Content(uid);
            if (content.Length > 0) return content;

            return PlaceholderText(uid);
        }

        public static string Description(this string uid)
        {
            return I18nUtils.Tr(uid + "/Description");
        }

        public static string PlaceholderText(this string uid)
        {
            return I18nUtils.Tr(uid + "/PlaceholderText");
        }

        public static string Content(this string uid)
        {
            return I18nUtils.Tr(uid + "/Content");
        }
    }
}
