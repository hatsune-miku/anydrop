// Copyright (c) Microsoft Corporation and Contributors.
// Licensed under the MIT License.

using AnyDrop.Bridge;
using AnyDrop.Extension;
using AnyDrop.Model;
using AnyDrop.Util;
using AnyDrop.ViewModel;
using AnyDrop.Worker;
using Microsoft.UI.Xaml;
using SRCounter;
using System;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Windows.Win32;
using Windows.Win32.Foundation;
using WinRT.Interop;

namespace AnyDrop.View
{
    public sealed partial class TrayIconHolderWindow : Window
    {
        private static SynchronizationContext context;
        private static FilePartWorker filePartWorker = new();

        public static TrayIconHolderWindow Instance { get; private set; }


        public TrayIconHolderWindow()
        {
            this.InitializeComponent();

            context = SynchronizationContext.Current;
            Instance = this;

            TrySignInAsync().FireAndForget();
            AnyDropBridge.TryStartAnyDropService();
            AnyDropBridge.SetOnTextReceivedHandler(OnTextReceived);
            AnyDropBridge.SetOnFileComingHandler(OnFileComing);
            AnyDropBridge.SetOnFileSendingHandler(OnFileSending);
            AnyDropBridge.SetOnFilePartHandler(OnFilePart);

            // Hide the window visually.
            AppWindow.Resize(new(1, 1));
            AppWindow.Move(new(32768, 32768));

            // Start workers.
            filePartWorker.Start();
        }

        private async Task TrySignInAsync()
        {
            if (!await AccountUtils.TryLoginWithSavedTokenAsync())
            {
                // Prompt to login if token expired.
                var window = new LoginWindow();
                window.Activate();
            }
        }

        // Called when a FilePartPacket is received.
        // In most cases, multiple calls will be made for a single file.
        // Return true to stop receiving the file.
        private static bool OnFilePart(byte fileId, UInt64 offset, UInt64 length, byte[] data)
        {
            try
            {
                NewFileViewModel remoteViewModel = GlobalViewModel.Instance.ReceiveFiles[fileId];
                var file = remoteViewModel.ReceivingFile;
                if (file == null || file.Status == AnyDropBridge.FileStatus.CancelledByReceiver || file.Status == AnyDropBridge.FileStatus.CancelledBySender)
                {
                    return true;
                }
            }
            catch (Exception)
            {
                return true;
            }

            filePartWorker.PostWorkload(new(fileId, offset, length, data));
            return false;
        }

        private static void OnFileSending(byte fileId, ulong progress, ulong total, AnyDropBridge.FileStatus status)
        {
            switch (status)
            {
                case AnyDropBridge.FileStatus.Rejected:
                    {
                        Debug.WriteLine("File rejected!");
                        break;
                    }
                case AnyDropBridge.FileStatus.Accepted:
                    {
                        Debug.WriteLine("File accepted!");
                        break;
                    }
                case AnyDropBridge.FileStatus.Completed:
                    {
                        Debug.WriteLine("File completed!");
                        break;
                    }
                case AnyDropBridge.FileStatus.Error:
                    {
                        Debug.WriteLine("File error!");
                        break;
                    }
                default:
                    {
                        break;
                    }
            }
        }

        private static void OnFileComing(ulong fileSize, string fileName, Peer peer)
        {
            context.Post((_) =>
            {
                var result = PInvoke.MessageBox(
                    HWND.Null,
                    $"File {fileName} from {peer} ({FileUtils.GetFileSizeDescription(fileSize)}) is coming! Receive?",
                    "Received File",
                    Windows.Win32.UI.WindowsAndMessaging.MESSAGEBOX_STYLE.MB_ICONINFORMATION
                    | Windows.Win32.UI.WindowsAndMessaging.MESSAGEBOX_STYLE.MB_YESNO
                );
                bool accept = result == Windows.Win32.UI.WindowsAndMessaging.MESSAGEBOX_RESULT.IDYES;
                byte fileId = FileUtils.NextFileId();
                if (accept)
                {
                    PrepareForReceiveFile(fileId, fileSize, fileName, peer);
                }
                AnyDropBridge.RespondToFile(
                    peer,
                    fileId,
                    fileSize,
                    fileName,
                    accept
                );
            }, null);
        }

        private static void OnTextReceived(string text, Peer peer)
        {
            if (AccountUtils.IsInBlockList(peer.IpAddress))
            {
                return;
            }

            context.Post(_ =>
            {
                try
                {
                    var window = NewTextWindow.Create(text, peer);
                    window.Activate();
                }
                catch (Exception e)
                {
                    Debug.WriteLine(e);
                }
            }, null);
        }

        private static void PrepareForReceiveFile(byte fileId, ulong fileSize, string fileName, Peer peer)
        {
            var savingFilename = FileUtils.GetFileName(fileName);
            var fullPath = Path.Join(SettingsUtils.String(Keys.SaveFilePath, ""), savingFilename);
            var directoryPath = Path.GetDirectoryName(fullPath);

            if (!Directory.Exists(directoryPath))
            {
                Directory.CreateDirectory(directoryPath);
            }
            var writingFileStream = File.Create(fullPath);

            var transferFile = new ReceiveFile
            {
                RemoteFullPath = fileName,
                WritingStream = writingFileStream,
                LocalSaveFullPath = fullPath,
                Progress = 0,
                TotalSize = fileSize,
                FileId = 1,
                Status = AnyDropBridge.FileStatus.Accepted,
                From = peer,
            };

            // Preallocate file size
            writingFileStream.SetLength((long)fileSize);

            // Enqueue
            // Open window
            context.Post((_) =>
            {
                GlobalViewModel.Instance.ReceiveFiles.TryAdd(fileId, new(transferFile));

                var window = new NewFileWindow(fileId);
                window.Activate();
            }, null);
        }

        private void Window_Activated(object sender, WindowActivatedEventArgs args)
        {
            IntPtr hwnd = WindowNative.GetWindowHandle(this);
            PInvoke.ShowWindow(new HWND(hwnd), Windows.Win32.UI.WindowsAndMessaging.SHOW_WINDOW_CMD.SW_HIDE);
        }
    }
}
