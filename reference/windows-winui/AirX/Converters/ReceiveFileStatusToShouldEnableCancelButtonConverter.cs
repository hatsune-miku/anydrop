using Microsoft.UI.Xaml.Data;
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace AnyDrop.Converters
{
    class ReceiveFileStatusToShouldEnableCancelButtonConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, string language)
        {
            var status = (AnyDropBridge.FileStatus) value;
            return status != AnyDropBridge.FileStatus.CancelledBySender
                && status != AnyDropBridge.FileStatus.CancelledByReceiver;
        }

        public object ConvertBack(object value, Type targetType, object parameter, string language)
        {
            throw new NotImplementedException();
        }
    }
}
