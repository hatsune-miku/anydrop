pub mod compatibility;
pub mod extension;
pub mod network;
pub mod packet;
pub mod proto;
pub mod service;
pub mod util;

pub mod transfer;

pub mod lib_generic;
pub mod lib_util;

/// Maximum clipboard payload in UTF-8 bytes (text) or encoded PNG bytes (image).
pub const MAX_CLIPBOARD_BYTES: usize = 4 * 1024 * 1024;
/// DataPacket (8 bytes) + TextPacket (6 bytes); excludes the TCP length prefix.
pub const MAX_CLIPBOARD_FRAME_BYTES: usize = MAX_CLIPBOARD_BYTES + 14;
