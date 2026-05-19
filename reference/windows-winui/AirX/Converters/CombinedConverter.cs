using Microsoft.UI.Xaml.Data;
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace AnyDrop.Converters
{
    class CombinedConverter : IValueConverter
    {
        public IValueConverter Converter1 { get; set; }
        public IValueConverter Converter2 { get; set; }

        public object Convert(object value, Type targetType, object parameter, string language)
        {
            var intermediate = Converter1.Convert(value, targetType, parameter, language);
            return Converter2.Convert(intermediate, targetType, parameter, language);
        }

        public object ConvertBack(object value, Type targetType, object parameter, string language)
        {
            var intermediate = Converter2.ConvertBack(value, targetType, parameter, language);
            return Converter1.ConvertBack(intermediate, targetType, parameter, language);
        }
    }
}
