use std::fmt;

#[derive(Debug, Clone)]
pub struct KeyValuePair {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct BTreeStatistics {
    pub tree_height: u32,
    pub internal_node_count: u64,
    pub leaf_node_count: u64,
    pub total_key_count: u64,
    pub avg_keys_per_leaf: f64,
    pub min_occupancy: f64,
    pub max_occupancy: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreeFormatIdentityError {
    ReservedVersion,
    ZeroMaxKeySize,
}

impl BTreeFormatIdentityError {
    pub const fn message(self) -> &'static str {
        match self {
            Self::ReservedVersion => "B-Tree key format version 0.0 is reserved",
            Self::ZeroMaxKeySize => "B-Tree key format max_key_size must not be zero",
        }
    }
}

impl fmt::Display for BTreeFormatIdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for BTreeFormatIdentityError {}

pub fn validate_btree_key_format_identity_parts(
    major: u32,
    minor: u32,
    max_key_size: u16,
) -> Result<(), BTreeFormatIdentityError> {
    if major == 0 && minor == 0 {
        return Err(BTreeFormatIdentityError::ReservedVersion);
    }
    if max_key_size == 0 {
        return Err(BTreeFormatIdentityError::ZeroMaxKeySize);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BTreeKeyFormatIdentity {
    pub major: u32,
    pub minor: u32,
    pub codec_version: u8,
    pub max_key_size: u16,
}

impl BTreeKeyFormatIdentity {
    pub const fn new(major: u32, minor: u32, codec_version: u8, max_key_size: u16) -> Self {
        Self {
            major,
            minor,
            codec_version,
            max_key_size,
        }
    }

    pub fn try_new(
        major: u32,
        minor: u32,
        codec_version: u8,
        max_key_size: u16,
    ) -> Result<Self, BTreeFormatIdentityError> {
        validate_btree_key_format_identity_parts(major, minor, max_key_size)?;
        Ok(Self {
            major,
            minor,
            codec_version,
            max_key_size,
        })
    }

    pub const V1_0: Self = Self {
        major: 1,
        minor: 0,
        codec_version: 1,
        max_key_size: 4096,
    };

    pub const fn is_backward_compatible_with(&self, other: BTreeKeyFormatIdentity) -> bool {
        self.major == other.major && self.minor >= other.minor
    }

    pub fn format_name(&self) -> String {
        format!("KeyV{}_Codec{}", self.major, self.codec_version)
    }
}

impl fmt::Display for BTreeKeyFormatIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BTreeKeyFormat(v{}.{}, codec={}, max_key={})",
            self.major, self.minor, self.codec_version, self.max_key_size
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreeOperationType {
    Lookup,
    RangeScan,
    Insert,
    Delete,
    Split,
    Merge,
}

impl BTreeOperationType {
    pub const fn is_read_only(&self) -> bool {
        matches!(self, Self::Lookup | Self::RangeScan)
    }

    pub const fn name(&self) -> &'static str {
        match self {
            Self::Lookup => "lookup",
            Self::RangeScan => "range_scan",
            Self::Insert => "insert",
            Self::Delete => "delete",
            Self::Split => "split",
            Self::Merge => "merge",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BTreeFormatIdentityError, BTreeKeyFormatIdentity, BTreeOperationType,
        validate_btree_key_format_identity_parts,
    };

    #[test]
    fn key_format_identity_rejects_reserved_zero_version() {
        assert_eq!(
            BTreeKeyFormatIdentity::try_new(0, 0, 1, 4096),
            Err(BTreeFormatIdentityError::ReservedVersion)
        );
        assert_eq!(
            validate_btree_key_format_identity_parts(0, 0, 4096),
            Err(BTreeFormatIdentityError::ReservedVersion)
        );
    }

    #[test]
    fn key_format_identity_rejects_zero_key_limit() {
        assert_eq!(
            BTreeKeyFormatIdentity::try_new(1, 0, 1, 0),
            Err(BTreeFormatIdentityError::ZeroMaxKeySize)
        );
        assert_eq!(
            validate_btree_key_format_identity_parts(1, 0, 0),
            Err(BTreeFormatIdentityError::ZeroMaxKeySize)
        );
    }

    #[test]
    fn key_format_identity_preserves_compatibility_rules() {
        let v1_0 = BTreeKeyFormatIdentity::V1_0;
        let v1_1 = BTreeKeyFormatIdentity::new(1, 1, 1, 4096);
        let v2_0 = BTreeKeyFormatIdentity::new(2, 0, 1, 4096);

        assert!(v1_1.is_backward_compatible_with(v1_0));
        assert!(!v1_0.is_backward_compatible_with(v1_1));
        assert!(!v2_0.is_backward_compatible_with(v1_0));
        assert_eq!(v1_0.format_name(), "KeyV1_Codec1");
    }

    #[test]
    fn operation_type_classifies_read_only_operations() {
        assert!(BTreeOperationType::Lookup.is_read_only());
        assert!(BTreeOperationType::RangeScan.is_read_only());
        assert!(!BTreeOperationType::Insert.is_read_only());
        assert_eq!(BTreeOperationType::Merge.name(), "merge");
    }
}
