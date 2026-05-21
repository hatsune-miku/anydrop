//! Wire protocol for transfer_v2.
//!
//! Control stream (bidirectional, client-initiated):
//!   [u32 BE length][JSON body]   length-prefixed JSON messages.
//!
//! Data streams (unidirectional, client-initiated, one per non-dir item):
//!   [u64 BE transfer_id][u32 BE item_idx][raw file bytes...]

use serde::{Deserialize, Serialize};

pub const ALPN: &[u8] = b"anydrop/1";
pub const DATA_HEADER_LEN: usize = 8 + 4; // transfer_id u64 + item_idx u32

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

/// Server's response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HelloAck {
    pub accepted: bool,
    /// Reason for rejection, when `accepted` is false.
    pub reject_reason: Option<String>,
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
