using AnyDrop.ViewModel;
using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace AnyDrop.Worker
{
    class FilePartWorker
    {
        private BlockingCollection<FilePartWorkload> queue = new();
        private SynchronizationContext context = SynchronizationContext.Current;
        private Thread worker;

        public FilePartWorker()
        {

        }

        public void PostWorkload(FilePartWorkload workload)
        {
            queue.Add(workload);
        }

        private void HandleSingleWorkload(FilePartWorkload workload)
        {
            NewFileViewModel remoteViewModel;
            try
            {
                remoteViewModel = GlobalViewModel.Instance.ReceiveFiles[workload.FileId];
            }
            catch (Exception)
            {
                Debug.WriteLine("Unexpected file incoming: fileid=" + workload.FileId + ", length=" + workload.Length);
                return;
            }

            var file = remoteViewModel.ReceivingFile;

            // User cancelled the file?
            if (file.Status == AnyDropBridge.FileStatus.CancelledByReceiver)
            {
                Debug.WriteLine("File cancelled!");
                file.WritingStream?.Close();
                GlobalViewModel.Instance.ReceiveFiles.Remove(workload.FileId);
                return;
            }

            file.Status = AnyDropBridge.FileStatus.InProgress;
            if (file.WritingStream == null)
            {
                file.Status = AnyDropBridge.FileStatus.Error;
                Debug.WriteLine("File not accepted!");
                return;
            }

            // Update UI.
            context.Post(_ =>
            {
                file.DisplayProgress = file.Progress;
                file.DisplayStatus = AnyDropBridge.FileStatus.InProgress;
            }, null);

            file.WritingStream.Seek((long)workload.Offset, SeekOrigin.Begin);
            file.WritingStream.Write(workload.Data, 0, (int)workload.Length);
            file.Progress += workload.Length;

            if (file.Progress == file.TotalSize)
            {
                context.Post(_ =>
                {
                    file.DisplayProgress = file.Progress;
                    file.DisplayStatus = AnyDropBridge.FileStatus.Completed;
                }, null);

                file.WritingStream.Close();
                file.WritingStream = null;
                file.Status = AnyDropBridge.FileStatus.Completed;
                Debug.WriteLine("File recv completed!");
            }
        }

        public void Run()
        {
            while (!queue.IsCompleted)
            {
                try
                {
                    var workload = queue.Take();
                    HandleSingleWorkload(workload);
                }
                catch (Exception e)
                {
                    Debug.WriteLine(e);
                }
            }
        }

        public void Start()
        {
            if (worker != null)
            {
                throw new InvalidOperationException("Worker already started.");
            }
            worker = new Thread(Run);
            worker.Start();
        }
    }

    public partial class FilePartWorkload
    {
        public byte FileId { get; set; }
        public UInt64 Offset { get; set; }
        public UInt64 Length { get; set; }
        public byte[] Data { get; set; }


        public FilePartWorkload(byte fileId, ulong offset, ulong length, byte[] data)
        {
            FileId = fileId;
            Offset = offset;
            Length = length;
            Data = data;
        }
    }
}
