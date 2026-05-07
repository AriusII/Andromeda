use super::validation::validate_identity_parts;
use andromeda_core::AndromedaResult;
use std::fmt;

/// Format identity for persisted B-Tree indexes.
///
/// Each index must record which key-format codec was used when it was created.
/// This allows later versions to validate format compatibility and reject
/// unsupported formats before mutation attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BTreeKeyFormatIdentity {
    /// Major version for breaking format changes.
    pub major: u32,
    /// Minor version for backward-compatible extensions.
    pub minor: u32,
    /// Codec version, where 1 is KeyV1 order-preserving encoding.
    pub codec_version: u8,
    /// Maximum key size in bytes for the persisted key format.
    pub max_key_size: u16,
}

impl BTreeKeyFormatIdentity {
    /// Create a compile-time format identity for constants and test fixtures.
    ///
    /// Runtime and durable decode paths should use [`Self::try_new`] when they
    /// need immediate typed validation. Validators still reject invalid
    /// identities before any operation, so this constructor never panics on
    /// malformed metadata.
    pub const fn new(major: u32, minor: u32, codec_version: u8, max_key_size: u16) -> Self {
        Self {
            major,
            minor,
            codec_version,
            max_key_size,
        }
    }

    /// Create a format identity from runtime or durable metadata.
    ///
    /// Decode and recovery paths must use this fallible constructor so corrupt
    /// format identity bytes produce typed storage errors instead of panics.
    pub fn try_new(
        major: u32,
        minor: u32,
        codec_version: u8,
        max_key_size: u16,
    ) -> AndromedaResult<Self> {
        validate_identity_parts(major, minor, max_key_size)?;
        Ok(Self {
            major,
            minor,
            codec_version,
            max_key_size,
        })
    }

    /// KeyV1 format identity locked by DEC-032.
    pub const V1_0: Self = Self {
        major: 1,
        minor: 0,
        codec_version: 1,
        max_key_size: 4096,
    };

    /// Check if this format is backward compatible with another format.
    pub const fn is_backward_compatible_with(&self, other: BTreeKeyFormatIdentity) -> bool {
        self.major == other.major && self.minor >= other.minor
    }

    /// Get a human-readable format identifier.
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
