/// A single slot directory entry (5 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotEntry {
    pub(crate) offset: u16,
    pub(crate) length: u16,
    pub(crate) flags: u8,
}

impl SlotEntry {
    pub const SIZE: usize = 5;

    pub fn new(offset: u16, length: u16) -> Self {
        Self {
            offset,
            length,
            flags: 0,
        }
    }

    pub fn mark_deleted(&mut self) {
        self.flags |= 0x01;
        self.offset = 0;
    }

    pub fn is_deleted(self) -> bool {
        (self.flags & 0x01) != 0
    }

    pub fn offset_if_live(self) -> Option<u16> {
        if self.is_deleted() {
            None
        } else {
            Some(self.offset)
        }
    }

    pub fn to_bytes(self) -> [u8; 5] {
        [
            (self.offset & 0xFF) as u8,
            ((self.offset >> 8) & 0xFF) as u8,
            (self.length & 0xFF) as u8,
            ((self.length >> 8) & 0xFF) as u8,
            self.flags,
        ]
    }

    pub fn from_bytes(bytes: [u8; 5]) -> Self {
        let offset = u16::from_le_bytes([bytes[0], bytes[1]]);
        let length = u16::from_le_bytes([bytes[2], bytes[3]]);
        let flags = bytes[4];
        Self {
            offset,
            length,
            flags,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_entry_serialization() {
        let entry = SlotEntry::new(100, 50);
        let bytes = entry.to_bytes();
        let deserialized = SlotEntry::from_bytes(bytes);

        assert_eq!(entry, deserialized);
    }

    #[test]
    fn test_slot_entry_deletion() {
        let mut entry = SlotEntry::new(100, 50);
        assert!(!entry.is_deleted());

        entry.mark_deleted();
        assert!(entry.is_deleted());
        assert_eq!(entry.offset, 0);
    }
}
