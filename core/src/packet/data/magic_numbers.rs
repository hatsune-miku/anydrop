//! Magic numbers carried in legacy `DataPacket` headers.
//!
//! As of the QUIC transfer migration, this enum only contains `Text`. File
//! transfer used to occupy 0x3939, 0x3941, 0x3942, 0x3943 — those values are
//! now reserved and the legacy data service drops anything it doesn't know.

pub enum MagicNumbers {
    Text,
}

impl MagicNumbers {
    pub fn value(&self) -> u16 {
        match self {
            MagicNumbers::Text => 0x3940,
        }
    }

    pub fn from(value: u16) -> Option<Self> {
        match value {
            0x3940 => Some(MagicNumbers::Text),
            _ => None,
        }
    }
}
