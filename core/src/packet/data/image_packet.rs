//! Clipboard image sync packet.
//!
//! Wire format is dead simple — PNG already has internal CRC32 chunks for
//! integrity, so we don't add our own hash on top:
//!
//! ```text
//!   4 bytes: u32 BE — PNG payload length
//!   N bytes: PNG-encoded image (RGBA → PNG done by the sender)
//! ```
//!
//! PNG sniff bytes (`89 50 4E 47 0D 0A 1A 0A`) at offset 4 give us a free
//! sanity check on deserialize.

use crate::compatibility::unified_endian::UnifiedEndian;
use crate::packet::protocol::serialize::Serialize;
use core::fmt;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};

const HEADER_SIZE: usize = 4;
/// Cap defensive — refuse images above this to avoid one peer flooding others.
const MAX_PNG_BYTES: usize = 64 * 1024 * 1024;
const PNG_MAGIC: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

pub enum ImagePacketError {
    InvalidData,
    NotPng,
    TooLarge,
}

impl Debug for ImagePacketError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)
    }
}

impl Display for ImagePacketError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::write(
            f,
            format_args!(
                "{}",
                match self {
                    ImagePacketError::InvalidData => "Invalid data.",
                    ImagePacketError::NotPng => "Payload is not a PNG.",
                    ImagePacketError::TooLarge => "PNG payload exceeds size cap.",
                }
            ),
        )
    }
}

impl Error for ImagePacketError {}

pub struct ImagePacket {
    png_bytes: Vec<u8>,
}

impl ImagePacket {
    pub fn new(png_bytes: Vec<u8>) -> Result<Self, ImagePacketError> {
        if png_bytes.len() > MAX_PNG_BYTES {
            return Err(ImagePacketError::TooLarge);
        }
        if png_bytes.len() < PNG_MAGIC.len() || png_bytes[..PNG_MAGIC.len()] != PNG_MAGIC {
            return Err(ImagePacketError::NotPng);
        }
        Ok(Self { png_bytes })
    }

    pub fn png_bytes(&self) -> &[u8] {
        &self.png_bytes
    }

    pub fn into_png_bytes(self) -> Vec<u8> {
        self.png_bytes
    }
}

impl Serialize<Vec<u8>, ImagePacketError> for ImagePacket {
    fn serialize(&self) -> Vec<u8> {
        let len = self.png_bytes.len() as u32;
        let mut out = Vec::with_capacity(HEADER_SIZE + self.png_bytes.len());
        out.extend_from_slice(&len.to_bytes());
        out.extend_from_slice(&self.png_bytes);
        out
    }

    fn deserialize(data: &Vec<u8>) -> Result<Self, ImagePacketError> {
        if data.len() < HEADER_SIZE {
            return Err(ImagePacketError::InvalidData);
        }
        let len = u32::from_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if len > MAX_PNG_BYTES {
            return Err(ImagePacketError::TooLarge);
        }
        if HEADER_SIZE + len > data.len() {
            return Err(ImagePacketError::InvalidData);
        }
        let png = data[HEADER_SIZE..HEADER_SIZE + len].to_vec();
        Self::new(png)
    }
}
