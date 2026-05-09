//! Storage format version management and compatibility matrix.
//!
//! This module defines durable storage format versions and compatibility rules
//! without depending on storage runtime implementation.

use std::cmp::Ordering;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// Unified version representation for all storage formats.
///
/// Versions follow semantic versioning:
/// - `major` increments on breaking changes (incompatible wire format)
/// - `minor` increments on backward-compatible extensions
///
/// **Invariant:** major and minor are never both zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FormatVersion {
    pub major: u32,
    pub minor: u32,
}

impl FormatVersion {
    /// Creates a compile-time format version for constants and test fixtures.
    ///
    /// Runtime and durable decode paths must use [`Self::try_new`] so corrupt
    /// metadata is reported through typed storage errors.
    ///
    /// # Panics
    /// Panics if both major and minor are zero (reserved for uninitialized state).
    pub const fn new(major: u32, minor: u32) -> Self {
        if major == 0 && minor == 0 {
            panic!("Version (0, 0) is reserved; use (1, 0) for initial release");
        }
        FormatVersion { major, minor }
    }

    /// Creates a format version from runtime or durable metadata.
    pub fn try_new(major: u32, minor: u32) -> AndromedaResult<Self> {
        if major == 0 && minor == 0 {
            return Err(format_version_error(
                "format version (0, 0) is reserved for uninitialized durable metadata",
            ));
        }
        Ok(FormatVersion { major, minor })
    }

    /// Current production versions (locked for Andromeda V0.5).
    pub const V1_0: Self = FormatVersion { major: 1, minor: 0 };

    /// Planned future versions for compatibility testing.
    pub const V1_5: Self = FormatVersion { major: 1, minor: 5 };
    pub const V2_0: Self = FormatVersion { major: 2, minor: 0 };

    /// Returns true if this version is backward compatible with `other`.
    pub fn is_backward_compatible_with(&self, other: FormatVersion) -> bool {
        self.major == other.major && self.minor >= other.minor
    }

    /// Returns true if `other` is forward compatible with this version.
    pub fn is_forward_compatible_with(&self, other: FormatVersion) -> bool {
        self.major == other.major && self.minor <= other.minor
    }

    /// Computes the compatibility relationship between writer and reader.
    pub fn compatibility_with(self, writer: FormatVersion) -> CompatibilityResult {
        if self.major != writer.major {
            return CompatibilityResult::Incompatible;
        }

        match self.minor.cmp(&writer.minor) {
            Ordering::Greater => CompatibilityResult::BackwardCompatible,
            Ordering::Equal => CompatibilityResult::FullyCompatible,
            Ordering::Less => CompatibilityResult::Incompatible,
        }
    }
}

/// Result of a compatibility check between two versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatibilityResult {
    /// Writer and reader versions are identical; full compatibility.
    FullyCompatible,

    /// Reader is newer and can read the writer format safely.
    BackwardCompatible,

    /// Incompatible; reader too old or major version mismatch.
    Incompatible,
}

impl CompatibilityResult {
    /// Returns true if reading is safe.
    pub fn is_safe_to_read(&self) -> bool {
        matches!(
            self,
            CompatibilityResult::FullyCompatible | CompatibilityResult::BackwardCompatible
        )
    }

    /// Returns human-readable description of the compatibility status.
    pub fn description(&self) -> &'static str {
        match self {
            CompatibilityResult::FullyCompatible => "Fully compatible (same version)",
            CompatibilityResult::BackwardCompatible => {
                "Backward compatible (reader is newer; can read safely)"
            },
            CompatibilityResult::Incompatible => {
                "Incompatible (version mismatch; upgrade reader or downgrade writer)"
            },
        }
    }
}

/// Enumeration of all durable storage format types in Andromeda.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageFormatKind {
    Page,
    HeapPage,
    BTreeKey,
    BTreeNode,
    WalRecord,
    WalPayload,
    Manifest,
    Segment,
    Checkpoint,
}

impl StorageFormatKind {
    /// Returns the current production version for this format.
    pub fn current_version(&self) -> FormatVersion {
        FormatVersion::V1_0
    }

    /// Returns all supported versions for this format.
    pub fn supported_versions(&self) -> &'static [FormatVersion] {
        &[FormatVersion::V1_0]
    }

    /// Returns a human-readable name for this format.
    pub fn name(&self) -> &'static str {
        match self {
            StorageFormatKind::Page => "Page",
            StorageFormatKind::HeapPage => "Heap Page",
            StorageFormatKind::BTreeKey => "B-Tree Key",
            StorageFormatKind::BTreeNode => "B-Tree Node",
            StorageFormatKind::WalRecord => "WAL Record",
            StorageFormatKind::WalPayload => "WAL Payload",
            StorageFormatKind::Manifest => "Backup Manifest",
            StorageFormatKind::Segment => "WAL Segment",
            StorageFormatKind::Checkpoint => "Checkpoint",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageFormatFingerprint {
    pub kind: StorageFormatKind,
    pub version: FormatVersion,
}

impl StorageFormatFingerprint {
    pub const fn new(kind: StorageFormatKind, version: FormatVersion) -> Self {
        Self { kind, version }
    }
}

/// Compatibility matrix for all supported format versions.
#[derive(Debug)]
pub struct CompatibilityMatrix {
    reader_version: FormatVersion,
}

impl CompatibilityMatrix {
    /// Creates a compatibility matrix for the current binary.
    pub fn new(reader_version: FormatVersion) -> Self {
        CompatibilityMatrix { reader_version }
    }

    /// Creates a matrix using the current production version (V1.0).
    pub fn production() -> Self {
        CompatibilityMatrix {
            reader_version: FormatVersion::V1_0,
        }
    }

    /// Checks if this reader can safely read data written by `writer_version`.
    pub fn can_read(&self, writer_version: FormatVersion) -> Result<(), String> {
        let compat = self.reader_version.compatibility_with(writer_version);
        match compat {
            CompatibilityResult::FullyCompatible | CompatibilityResult::BackwardCompatible => {
                Ok(())
            },
            CompatibilityResult::Incompatible => Err(format!(
                "Format version mismatch: reader {} cannot read writer {} ({})",
                format_version_string(self.reader_version),
                format_version_string(writer_version),
                compat.description()
            )),
        }
    }

    /// Returns the compatibility result without consuming the error.
    pub fn check_compatibility(&self, writer_version: FormatVersion) -> CompatibilityResult {
        self.reader_version.compatibility_with(writer_version)
    }
}

fn format_version_string(version: FormatVersion) -> String {
    format!("{}.{}", version.major, version.minor)
}

fn format_version_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
