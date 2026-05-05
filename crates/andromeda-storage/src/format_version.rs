#![forbid(unsafe_code)]

//! Storage format version management and compatibility matrix.
//!
//! This module defines all durable storage format versions and ensures that
//! readers and writers negotiate compatible versions. It implements the
//! backward/forward compatibility rules from STOR-FND specification.

use std::cmp::Ordering;

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
    /// Creates a new format version.
    ///
    /// # Panics
    /// Panics if both major and minor are zero (reserved for uninitialized state).
    pub const fn new(major: u32, minor: u32) -> Self {
        if major == 0 && minor == 0 {
            panic!("Version (0, 0) is reserved; use (1, 0) for initial release");
        }
        FormatVersion { major, minor }
    }

    /// Current production versions (locked for Andromeda V0.5).
    pub const V1_0: Self = FormatVersion { major: 1, minor: 0 };

    /// Planned future versions for compatibility testing.
    pub const V1_5: Self = FormatVersion { major: 1, minor: 5 };
    pub const V2_0: Self = FormatVersion { major: 2, minor: 0 };

    /// Returns true if this version is backward compatible with `other`.
    ///
    /// Backward compatibility means: code written for `other` can be read
    /// by code built for `self`.
    ///
    /// # Rules
    /// - Same major version required
    /// - `other.minor <= self.minor` (reader has all writer's features)
    ///
    /// # Examples
    /// ```
    /// assert!(V1_5.is_backward_compatible_with(V1_0));  // Reader 1.5 reads 1.0
    /// assert!(!V1_0.is_backward_compatible_with(V1_5)); // Reader 1.0 cannot read 1.5
    /// assert!(!V2_0.is_backward_compatible_with(V1_0)); // Major version mismatch
    /// ```
    pub fn is_backward_compatible_with(&self, other: FormatVersion) -> bool {
        self.major == other.major && self.minor >= other.minor
    }

    /// Returns true if `other` is forward compatible with this version.
    ///
    /// Forward compatibility means: code written for `self` can be read
    /// by code built for `other`.
    ///
    /// # Rules
    /// - Same major version required
    /// - `self.minor <= other.minor` (writer version must be <= reader version)
    ///
    /// # Examples
    /// ```
    /// assert!(V1_0.is_forward_compatible_with(V1_5));   // Writer 1.0 reads by 1.5
    /// assert!(!V1_5.is_forward_compatible_with(V1_0));  // Writer 1.5 too new for 1.0
    /// assert!(!V1_0.is_forward_compatible_with(V2_0));  // Major version mismatch
    /// ```
    pub fn is_forward_compatible_with(&self, other: FormatVersion) -> bool {
        self.major == other.major && self.minor <= other.minor
    }

    /// Computes the compatibility relationship between writer and reader.
    ///
    /// # Returns
    /// - `CompatibilityResult::FullyCompatible` — can read/write safely
    /// - `CompatibilityResult::BackwardCompatible` — reader has extra features; OK
    /// - `CompatibilityResult::Incompatible` — cannot read; version too new or major mismatch
    ///
    /// # Examples
    /// ```
    /// let writer = V1_0;
    /// let reader = V1_5;
    /// assert_eq!(
    ///     reader.compatibility_with(writer),
    ///     CompatibilityResult::BackwardCompatible
    /// );
    /// ```
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

    /// Reader is newer; has all writer features; can read safely.
    /// (e.g., reader 1.5 reading data written by 1.0)
    BackwardCompatible,

    /// Incompatible; reader too old or major version mismatch.
    /// Must upgrade reader or downgrade writer.
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
            }
            CompatibilityResult::Incompatible => {
                "Incompatible (version mismatch; upgrade reader or downgrade writer)"
            }
        }
    }
}

/// Enumeration of all durable storage format types in Andromeda.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageFormatKind {
    Page,
    WalRecord,
    Manifest,
    Segment,
    Checkpoint,
}

impl StorageFormatKind {
    /// Returns the current production version for this format.
    pub fn current_version(&self) -> FormatVersion {
        // All formats are currently at V1.0 (Andromeda V0.5 release)
        FormatVersion::V1_0
    }

    /// Returns all supported versions for this format (for testing).
    pub fn supported_versions(&self) -> &'static [FormatVersion] {
        &[FormatVersion::V1_0]
    }

    /// Returns a human-readable name for this format.
    pub fn name(&self) -> &'static str {
        match self {
            StorageFormatKind::Page => "Page",
            StorageFormatKind::WalRecord => "WAL Record",
            StorageFormatKind::Manifest => "Backup Manifest",
            StorageFormatKind::Segment => "WAL Segment",
            StorageFormatKind::Checkpoint => "Checkpoint",
        }
    }
}

/// Compatibility matrix for all supported format versions.
///
/// This is the authoritative source of truth for version compatibility.
/// It enforces the rules from STOR-FND specification section 8.
#[derive(Debug)]
pub struct CompatibilityMatrix {
    /// Current reader version (what this binary supports)
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
    ///
    /// # Returns
    /// - `Ok(())` if reading is safe
    /// - `Err(reason)` if versions are incompatible
    ///
    /// # Examples
    /// ```
    /// let matrix = CompatibilityMatrix::production();
    /// matrix.can_read(V1_0)?;  // OK
    /// matrix.can_read(V1_5)?;  // OK (backward compatible)
    /// matrix.can_read(V2_0)?;  // Err (too new)
    /// ```
    pub fn can_read(&self, writer_version: FormatVersion) -> Result<(), String> {
        let compat = self.reader_version.compatibility_with(writer_version);
        match compat {
            CompatibilityResult::FullyCompatible | CompatibilityResult::BackwardCompatible => {
                Ok(())
            }
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

/// Helper function to format a version as "major.minor".
fn format_version_string(version: FormatVersion) -> String {
    format!("{}.{}", version.major, version.minor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_creation() {
        let v1_0 = FormatVersion::V1_0;
        assert_eq!(v1_0.major, 1);
        assert_eq!(v1_0.minor, 0);
    }

    #[test]
    fn test_backward_compatibility() {
        assert!(FormatVersion::V1_5.is_backward_compatible_with(FormatVersion::V1_0));
        assert!(!FormatVersion::V1_0.is_backward_compatible_with(FormatVersion::V1_5));
        assert!(!FormatVersion::V1_5.is_backward_compatible_with(FormatVersion::V2_0));
    }

    #[test]
    fn test_forward_compatibility() {
        assert!(FormatVersion::V1_0.is_forward_compatible_with(FormatVersion::V1_5));
        assert!(!FormatVersion::V1_5.is_forward_compatible_with(FormatVersion::V1_0));
        assert!(!FormatVersion::V1_0.is_forward_compatible_with(FormatVersion::V2_0));
    }

    #[test]
    fn test_compatibility_matrix() {
        let matrix_1_0 = CompatibilityMatrix::new(FormatVersion::V1_0);

        // Same version: fully compatible
        let result = matrix_1_0.check_compatibility(FormatVersion::V1_0);
        assert_eq!(result, CompatibilityResult::FullyCompatible);

        // Writer older than reader (backward compat): OK
        let result = matrix_1_0.check_compatibility(FormatVersion::V1_0);
        assert!(result.is_safe_to_read());

        // Writer newer than reader (V2.0): incompatible
        let result = matrix_1_0.check_compatibility(FormatVersion::V2_0);
        assert_eq!(result, CompatibilityResult::Incompatible);
        assert!(!result.is_safe_to_read());
    }

    #[test]
    fn test_can_read_valid() {
        let matrix = CompatibilityMatrix::production();
        assert!(matrix.can_read(FormatVersion::V1_0).is_ok());
    }

    #[test]
    fn test_can_read_invalid() {
        let matrix = CompatibilityMatrix::production();
        let err = matrix.can_read(FormatVersion::V2_0);
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("mismatch"));
    }

    #[test]
    fn test_storage_format_kind_current_version() {
        assert_eq!(
            StorageFormatKind::Page.current_version(),
            FormatVersion::V1_0
        );
        assert_eq!(
            StorageFormatKind::WalRecord.current_version(),
            FormatVersion::V1_0
        );
        assert_eq!(
            StorageFormatKind::Manifest.current_version(),
            FormatVersion::V1_0
        );
    }

    #[test]
    fn test_compatibility_matrix_all_versions() {
        // Test the complete compatibility matrix as specified in STOR-FND
        let matrix_1_0 = CompatibilityMatrix::new(FormatVersion::V1_0);
        let matrix_1_5 = CompatibilityMatrix::new(FormatVersion::V1_5);
        let matrix_2_0 = CompatibilityMatrix::new(FormatVersion::V2_0);

        // Reader 1.0
        assert!(matrix_1_0.can_read(FormatVersion::V1_0).is_ok()); // 1.0 → 1.0
        assert!(matrix_1_0.can_read(FormatVersion::V1_5).is_err()); // 1.5 → 1.0 (too new)

        // Reader 1.5
        assert!(matrix_1_5.can_read(FormatVersion::V1_0).is_ok()); // 1.0 → 1.5
        assert!(matrix_1_5.can_read(FormatVersion::V1_5).is_ok()); // 1.5 → 1.5
        assert!(matrix_1_5.can_read(FormatVersion::V2_0).is_err()); // 2.0 → 1.5 (too new)

        // Reader 2.0
        assert!(matrix_2_0.can_read(FormatVersion::V1_0).is_err()); // 1.0 → 2.0 (major mismatch)
        assert!(matrix_2_0.can_read(FormatVersion::V1_5).is_err()); // 1.5 → 2.0 (major mismatch)
        assert!(matrix_2_0.can_read(FormatVersion::V2_0).is_ok()); // 2.0 → 2.0
    }

    #[test]
    fn test_format_version_ordering() {
        assert!(FormatVersion::V1_0 < FormatVersion::V1_5);
        assert!(FormatVersion::V1_5 < FormatVersion::V2_0);
        assert!(FormatVersion::V1_0 < FormatVersion::V2_0);
    }

    #[test]
    fn test_compatibility_result_descriptions() {
        assert!(
            !CompatibilityResult::FullyCompatible
                .description()
                .is_empty()
        );
        assert!(
            !CompatibilityResult::BackwardCompatible
                .description()
                .is_empty()
        );
        assert!(!CompatibilityResult::Incompatible.description().is_empty());
    }
}
