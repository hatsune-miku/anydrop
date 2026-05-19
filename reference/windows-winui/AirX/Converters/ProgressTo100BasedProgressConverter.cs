using Microsoft.UI.Xaml.Data;
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace AnyDrop.Converters
{
    public class ProgressTo100BasedProgressConverter : IValueConverter
    {
        public UInt64 TotalSize { get; set; }

        public object Convert(object value, Type targetType, object parameter, string language)
        {
            UInt64 progress = (UInt64)value;
            Debug.WriteLine("Progress in UI: " + progress);
            return GetProgressOutOf100(progress);
        }

        public object ConvertBack(object value, Type targetType, object parameter, string language)
        {
            throw new NotImplementedException();
        }

        public double GetProgressOutOf100(UInt64 progress)
        {
            try
            {
                var percentage = 1.0 * progress / TotalSize;
                var hundredBasedProgress = (int)(percentage * 100);
                return hundredBasedProgress;
            }
            catch
            {
                return 0;
            }
        }
    }
}
