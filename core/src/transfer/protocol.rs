//! Wire protocol for the QUIC `transfer` module.
//!
//! Control stream (bidirectional, client-initiated):
//!   `[u32 BE length][JSON body]` — length-prefixed JSON messages.
//!
//! Data streams (unidirectional, client-initiated, one per non-dir item):
//!   `[u64 BE transfer_id][u32 BE item_idx][u64 BE start_offset][raw bytes…]`
//!
//! The `start_offset` lets the sender resume an interrupted transfer: on
//! reconnect, the receiver tells the sender how many bytes per item it
//! already has via [`HelloAck::resume_offsets`], and the sender skips ahead
//! in its source file before streaming the remainder.

use serde::{Deserialize, Serialize};

pub const ALPN: &[u8] = b"anydrop/1";

/// QUIC application close codes used when a side tears down a connection.
///
/// These let the *peer* distinguish a graceful completion from a terminal
/// cancel from a pause. The pause code is what makes the pause feature work:
/// when a side sees the connection closed with [`CLOSE_PAUSE`] it must
/// preserve resume state and report `Paused` rather than routing the drop
/// through the error/abort path.
///
/// Normal completion — what a clean `AllDone` close uses.
pub const CLOSE_DONE: u32 = 0;
/// Terminal cancel — peer should treat the transfer as aborted, no resume.
pub const CLOSE_CANCEL: u32 = 1;
/// Pause — peer must preserve resume state, report `Paused`, and NOT error.
pub const CLOSE_PAUSE: u32 = 2;

/// Header length on every unidirectional data stream.
pub const DATA_HEADER_LEN: usize = 8 + 4 + 8;

/// A single file or directory in a transfer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    /// Path relative to the transfer root (forward slashes).
    pub rel_path: String,
    /// File size in bytes. Zero for directories.
    pub size: u64,
    /// True if this entry is a directory.
    pub is_dir: bool,
}

/// First message client sends on the control stream.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hello {
    pub transfer_id: u64,
    pub display_name: String,
    pub items: Vec<Item>,
}

/// The very first control-stream message from a connecting client. It is
/// either a fresh/resumed send offer (`Hello`) or a request asking the peer
/// (which is the *sender* of `transfer_id`) to reconnect and resume — used
/// when the RECEIVER initiated the pause and now wants to resume, since only
/// the sender holds the `send_args` needed to reconnect.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ClientIntro {
    Hello(Hello),
    Resume { transfer_id: u64 },
}

/// Server's response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HelloAck {
    pub accepted: bool,
    /// Reason for rejection, when `accepted` is false.
    pub reject_reason: Option<String>,
    /// On reconnect for an in-flight transfer: bytes the receiver already has
    /// for each previously-started item, keyed by `item_idx`. Items absent
    /// from this list start fresh from offset 0.
    #[serde(default)]
    pub resume_offsets: Vec<(u32, u64)>,
}

/// Server → client progress / completion messages on the control stream.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Status {
    /// Acknowledges that an item has been fully received and written to disk.
    ItemDone { item_idx: u32 },
    /// All items received successfully.
    AllDone,
    /// Server is aborting the transfer.
    Abort { reason: String },
}
