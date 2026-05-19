using Microsoft.UI.Xaml.Data;
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace AnyDrop.Converters
{
    class UInt64ToDoubleConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, string language)
        {
            ulong sizeInBytes = (ulong)value;
            return (double)sizeInBytes;
        }

        public object ConvertBack(object value, Type targetType, object parameter, string language)
        {
            double sizeInBytes = (double)value;
            return (ulong)sizeInBytes;
        }
    }
}
