using Microsoft.UI.Xaml.Data;
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace AnyDrop.Converters
{
    public class ProgressToStopOrOpenTextConverter : IValueConverter
    {
        public UInt64 TotalSize { get; set; }

        public object Convert(object value, Type targetType, object parameter, string language)
        {
            UInt64 progress = (UInt64)value;
            if (progress == TotalSize)
            {
                return "OPEN FOLDER";
            }
            else
            {
                return "STOP";
            }
        }

        public object ConvertBack(object value, Type targetType, object parameter, string language)
        {
            throw new NotImplementedException();
        }
    }
}
