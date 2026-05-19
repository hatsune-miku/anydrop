using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace AnyDrop.Model
{
    public class Peer
    {
        public string Hostname { get; set; }
        public string IpAddress { get; set; }
        public int Port { get; set; }

        public static readonly Peer Sample = new()
        {
            Hostname = "chance",
            IpAddress = "10.0.0.6",
            Port = 12345,
        };

        /// <summary>
        /// Peer string format: <hostname>@<ip>:<port>
        /// </summary>
        public static Peer Parse(string s)
        {
            // Incomplete peer string?
            if (!s.Contains("@"))
            {
                s = "<empty>@" + s;
            }

            var part1 = s.Split("@");
            var part2 = part1[1].Split(":");
            return new Peer
            {
                Hostname = part1[0],
                IpAddress = part2[0],
                Port = int.Parse(part2[1]),
            };
        }

        public static bool TryParse(string s, out Peer peer)
        {
            try
            {
                peer = Parse(s);
                return true;
            }
            catch (Exception)
            {
                peer = null;
                return false;
            }
        }

        public static Peer FromUid(int uid)
        {
            return new Peer
            {
                Hostname = $"UID={uid}",
                IpAddress = "AnyDrop Kafka Cluster",
                Port = 0,
            };
        }

        public override string ToString()
        {
            return $"{Hostname}@{IpAddress}:{Port}";
        }
    }
}
