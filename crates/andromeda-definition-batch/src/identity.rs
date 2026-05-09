/// Unique identifier for a DefinitionBatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DefinitionBatchId(u64);

impl DefinitionBatchId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

/// Administrative import attempt that submitted a DefinitionBatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DefinitionBatchImportId(u64);

impl DefinitionBatchImportId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

/// Canonical SHA-256 digest of the ordered DefinitionBatch source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DefinitionBatchSourceHash([u8; Self::LEN]);

impl DefinitionBatchSourceHash {
    pub const LEN: usize = 32;

    pub const fn new(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_batch_id_preserves_value() {
        let batch_id = DefinitionBatchId::new(42);

        assert_eq!(batch_id.get(), 42);
        assert!(!batch_id.is_zero());
        assert!(DefinitionBatchId::default().is_zero());
    }

    #[test]
    fn definition_batch_import_id_preserves_value() {
        let import_id = DefinitionBatchImportId::new(77);

        assert_eq!(import_id.get(), 77);
        assert!(!import_id.is_zero());
        assert!(DefinitionBatchImportId::default().is_zero());
    }

    #[test]
    fn source_hash_preserves_bytes_and_zero_state() {
        let zero = DefinitionBatchSourceHash::default();
        assert!(zero.is_zero());
        assert_eq!(zero.as_bytes(), [0; DefinitionBatchSourceHash::LEN]);

        let hash = DefinitionBatchSourceHash::new([7; DefinitionBatchSourceHash::LEN]);
        assert!(!hash.is_zero());
        assert_eq!(hash.as_bytes(), [7; DefinitionBatchSourceHash::LEN]);
    }
}
