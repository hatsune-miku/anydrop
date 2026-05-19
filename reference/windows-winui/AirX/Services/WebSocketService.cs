using AnyDrop.Model;
using AnyDrop.ViewModel;
using System;
using System.Diagnostics;
using System.Net.WebSockets;
using System.Threading.Tasks;
using Websocket.Client;

namespace AnyDrop.Services
{
    class WebSocketService: IObserver<ResponseMessage>, IObserver<DisconnectionInfo>, IObserver<ReconnectionInfo>
    {
        private WebsocketClient _ws;

        public static WebSocketService Instance { get; } = new();

        private WebSocketService()
        {

        }

        public async Task<bool> InitializeAsync()
        {
            if (_ws != null)
            {
                await _ws.Stop(WebSocketCloseStatus.NormalClosure, "");
            }

            // Ensure logged in.
            if (!GlobalViewModel.Instance.IsSignedIn)
            {
                return false;
            }

            _ws = new(new(AnyDropCloud.WebSocketBaseUrl));

            Debug.WriteLine("WebSocketService: Connecting to server...");
            await _ws.Start();

            _ws.MessageReceived.Subscribe(this);
            _ws.DisconnectionHappened.Subscribe(this);
            _ws.ReconnectionHappened.Subscribe(this);

            _ws.Send(GlobalViewModel.Instance.LoggingInUid.ToString());
            Debug.WriteLine("WebSocketService: Connected to server.");

            return true;
        }

        private void OnBinaryReceived(byte[] data)
        {
            Message message;
            try
            {
                message = Message.Parse(data);
            }
            catch (Exception)
            {
                Debug.WriteLine("Failed to parse message.");
                return;
            }
            
            switch (message.MessageTypeInfo)
            {
                case MessageType.Text:
                    AnyDropBridge.OnTextReceived(message.RawContent, Peer.FromUid(message.SenderUid));
                    break;

                case MessageType.FileUrl:
                    break;
            }
        }

        public void OnNext(ResponseMessage value)
        {
            switch (value.MessageType)
            {
                case WebSocketMessageType.Text:
                    Debug.WriteLine("Unexpected text message received: " + value.Text);
                    break;
 
                case WebSocketMessageType.Binary:
                    OnBinaryReceived(value.Binary);
                    break;

                default:
                    break;
            };
        }

        public void OnNext(ReconnectionInfo value)
        {
            _ws.Send(GlobalViewModel.Instance.LoggingInUid.ToString());
            Debug.WriteLine("WebSocketService: Reconnected to server.");
        }

        public void OnNext(DisconnectionInfo value)
        {
            Debug.WriteLine("WebSocketService: Disconnected from server.");
        }

        public void OnCompleted()
        {
            // Left blank.
        }

        public void OnError(Exception error)
        {
            Debug.WriteLine("Error in WebSocketService: " + error.Message);
        }

    }
}
