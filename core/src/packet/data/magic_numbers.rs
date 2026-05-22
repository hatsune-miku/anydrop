//! Magic numbers carried in legacy `DataPacket` headers.
//!
//! As of the QUIC transfer migration, only clipboard payloads ride this code
//! path: Text (0x3940) for strings, Image (0x3944) for screenshot/copied
//! image data. File transfer used to live here too (0x3939, 0x3941, 0x3942,
//! 0x3943) — those values are now reserved and the legacy data service
//! drops anything it doesn't know.

pub enum MagicNumbers {
    Text,
    Image,
}

impl MagicNumbers {
    pub fn value(&self) -> u16 {
        match self {
            MagicNumbers::Text => 0x3940,
            MagicNumbers::Image => 0x3944,
        }
    }

    pub fn from(value: u16) -> Option<Self> {
        match value {
            0x3940 => Some(MagicNumbers::Text),
            0x3944 => Some(MagicNumbers::Image),
            _ => None,
        }
    }
}
