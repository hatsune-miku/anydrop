using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace AnyDrop.Model
{
    // 4 Bytes: sender uid
    // 4 Bytes: message type
    // 4 Bytes: message length
    // N Bytes: message content
    public class Message
    {
        public int SenderUid { get; set; }
        public MessageType MessageTypeInfo { get; set; }
        public string RawContent { get; set; }

        public Message(int senderUid, MessageType messageType, string rawContent)
        {
            SenderUid = senderUid;
            MessageTypeInfo = messageType;
            RawContent = rawContent;
        }

        public static Message Parse(byte[] data)
        {
            if (data.Length < 12) // check if data has at least 12 bytes
            {
                throw new ArgumentException("Data is too short to represent a Message.");
            }

            int senderUid = BitConverter.ToInt32(
                BitConverter.IsLittleEndian 
                ? data.Take(4).Reverse().ToArray() 
                : data.Take(4).ToArray(), 0);
            int messageType = BitConverter.ToInt32(
                BitConverter.IsLittleEndian 
                ? data.Skip(4).Take(4).Reverse().ToArray()
                : data.Skip(4).Take(4).ToArray(), 0);
            int messageLength = BitConverter.ToInt32(
                BitConverter.IsLittleEndian 
                ? data.Skip(8).Take(4).Reverse().ToArray() 
                : data.Skip(8).Take(4).ToArray(), 0);

            if (data.Length < 12 + messageLength) // check if data has enough bytes for the message
            {
                throw new ArgumentException("Data is too short for the given message length.");
            }

            string rawContent = Encoding.UTF8.GetString(data, 12, messageLength);

            MessageType type = (MessageType)messageType; // Assuming MessageType is an enum

            return new Message(senderUid, type, rawContent);
        }

    }

    public enum MessageType: int
    {
        Text = 1,
        FileUrl = 2
    }
}
