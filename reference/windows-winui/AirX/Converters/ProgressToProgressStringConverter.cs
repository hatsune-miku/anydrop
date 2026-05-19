using Microsoft.UI.Xaml.Data;
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace AnyDrop.Converters
{
    public class ProgressToProgressStringConverter : IValueConverter
    {
        public UInt64 TotalSize { get; set; }

        public object Convert(object value, Type targetType, object parameter, string language)
        {
            UInt64 progress = (UInt64)value;
            return GetProgressString(progress);
        }

        public object ConvertBack(object value, Type targetType, object parameter, string language)
        {
            throw new NotImplementedException();
        }


        public string GetProgressString(UInt64 progress)
        {
            try
            {
                return string.Format("{0:N2}%", 1.0 * progress / TotalSize * 100);
            }
            catch
            {
                return "(Processing)";
            }
        }

    }
}
