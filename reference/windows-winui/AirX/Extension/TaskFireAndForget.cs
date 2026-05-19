using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace AnyDrop.Extension
{
    public static class TaskFireAndForget
    {
        public static void FireAndForget(this Task task)
        {
            _ = task.ContinueWith(it =>
            {
                Debug.WriteLineIf(it.IsFaulted, it.Exception);
            }, TaskScheduler.Default);
        }
    }
}
