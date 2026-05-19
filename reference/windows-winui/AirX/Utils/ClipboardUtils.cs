using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Windows.ApplicationModel.DataTransfer;

namespace AnyDrop.Util
{
    class ClipboardUtils
    {
        public static async Task<string> GetTextAsync()
        {
            var packageView = Clipboard.GetContent();
            try
            {
                return await packageView.GetTextAsync();
            }
            catch (Exception ex)
            {
                Debug.WriteLine(ex);
                return "";
            }
        }

        public static void SetText(string newText)
        {
            try
            {
                var package = new DataPackage();
                package.SetText(newText);
                Clipboard.SetContent(package);
            }
            catch (Exception) { }

        }
    }
}
