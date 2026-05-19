using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading.Tasks;
using static AnyDropBridge;

namespace AnyDrop.Bridge
{
    // Source: bridge.h
    internal class AnyDropNative
    {
        const string DLL_NAME = "libanydrop.dll";

        [DllImport(DLL_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern int anydrop_version();

        [DllImport(DLL_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern int anydrop_compatibility_number();

        [DllImport(DLL_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void anydrop_init();

        [DllImport(DLL_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr anydrop_create(
            UInt16 discovery_service_server_port,
            UInt16 discovery_service_client_port,
            IntPtr text_service_listen_addr,
            UInt32 text_service_listen_addr_len,
            UInt16 text_service_listen_port,
            UInt32 group_identity
        );

        [DllImport(DLL_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern bool anydrop_lan_broadcast(IntPtr anydropPtr);

        [DllImport(DLL_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint anydrop_get_peers(IntPtr anydropPtr, IntPtr buffer);

        [DllImport(DLL_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern UInt64 anydrop_version_string(IntPtr buffer);

        [DllImport(DLL_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void anydrop_send_text(
            IntPtr anydropPtr,
            string host,
            uint host_len,
            IntPtr text,
            uint text_len
        );

        [DllImport(DLL_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void anydrop_broadcast_text(
            IntPtr anydropPtr,
            IntPtr text,
            uint len
        );

        [DllImport(DLL_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void anydrop_lan_discovery_service(
            IntPtr anydropPtr,
            InterruptFunc should_interrupt
        );


        [DllImport(DLL_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void anydrop_data_service(
            IntPtr anydropPtr,
            TextCallbackFunction textCallback,
            FileComingCallbackFunction fileComingCallback,
            FileSendingCallbackFunction fileSendingCallback,
            FilePartCallbackFunction filePartCallback,
            InterruptFunc interruptCallback
        );

        [DllImport(DLL_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void anydrop_respond_to_file(
            IntPtr anydropPtr,
            IntPtr host,
            uint host_len,
            byte file_id,
            UInt64 file_size,
            IntPtr file_path,
            uint file_path_len,
            bool accept
        );

        [DllImport(DLL_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void anydrop_try_send_file(
            IntPtr anydropPtr,
            IntPtr host,
            uint host_len,
            IntPtr file_path,
            uint file_path_len
        );
    }
}
