using AnyDrop.Utils;
using Microsoft.UI.Xaml.Data;
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace AnyDrop.Converters
{
    public class ServiceOnlineStatusToDescriptionConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, string language)
        {
            bool isOnline = (bool)value;
            return isOnline
                ? "AnyDropIsOnline".Text()
                : "AnyDropIsOffline".Text();
        }

        public object ConvertBack(object value, Type targetType, object parameter, string language)
        {
            throw new NotImplementedException();
        }
    }
}
