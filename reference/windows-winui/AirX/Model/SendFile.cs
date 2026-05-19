using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace AnyDrop.Model
{
    public class SendFile
    {
        public string Filename { get; set; }
        public string FullPath { get; set; }
        public ulong Progress { get; set; }
        public ulong TotalSize { get; set; }
        public int FileId { get; set; }
        public AnyDropBridge.FileStatus Status { get; set; }
    }
}
