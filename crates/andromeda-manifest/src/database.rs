use andromeda_error::AndromedaResult;
use andromeda_wal::Lsn;

use crate::codec::{MANIFEST_ENCODED_SIZE, decode_manifest, encode_manifest};
use crate::{ManifestDurabilityBoundary, StorageFormatManifest};

/// An in-memory domain record describing a durable database manifest.
///
/// This type is the canonical in-memory representation.  Persistent storage
/// and network serialisation must always go through [`DatabaseManifest::encode`]
/// and [`DatabaseManifest::decode`] — never through native Rust struct layout.
///
/// ## Format history
///
/// | Format version | Encoded size | Notes                              |
/// |---|---|---|
/// | 1 (W3)         | 96 bytes     | No locator fields                  |
/// | 2 (W4)         | 112 bytes    | Added `segment_index_file_id` and `btree_root_page_id` |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatabaseManifest {
    /// Stable database identity — must be non-zero.
    pub database_id: u64,
    /// Monotonically increasing generation counter for the manifest chain.
    pub manifest_version: u64,
    /// Cold-snapshot anchor — ties this manifest to a published snapshot.
    pub snapshot_id: u64,
    /// Last completed WAL checkpoint LSN.
    pub base_checkpoint_lsn: Lsn,
    /// Minimum WAL start LSN for recovery (recovery floor).
    pub required_wal_start_lsn: Lsn,
    /// SHA-256 / hash of the previous manifest's encoded bytes (hash chain).
    pub previous_manifest_hash: [u8; 32],
    /// Domain CRC carried through the WAL switch payload.
    /// Must be non-zero (enforced by [`ManifestDurabilityBoundary::validate`]).
    pub manifest_crc: u32,
    /// File-system identifier for the durable `SegmentIndexV0` file associated
    /// with this manifest's cold snapshot.  Zero means no segment index has
    /// been published yet (bootstrap / first-run databases).
    ///
    /// Startup must decode the file identified by this id **before** the WAL
    /// scan to satisfy the P05 exit criterion:
    /// *"Startup lit root + manifest + SegmentIndex + WAL tail, pas tout le ColdStore."*
    pub segment_index_file_id: u64,
    /// Page-id of the B+Tree root page for the primary cold B+Tree structure.
    /// Zero means no root has been published yet.
    pub btree_root_page_id: u64,
}

impl DatabaseManifest {
    /// Project to the durable boundary view used by recovery fences.
    #[must_use]
    pub const fn durability_boundary(self) -> ManifestDurabilityBoundary {
        ManifestDurabilityBoundary {
            database_id: self.database_id,
            manifest_version: self.manifest_version,
            snapshot_id: self.snapshot_id,
            base_checkpoint_lsn: self.base_checkpoint_lsn,
            required_wal_start_lsn: self.required_wal_start_lsn,
            previous_manifest_hash: self.previous_manifest_hash,
            manifest_crc: self.manifest_crc,
        }
    }

    /// Recovery floor LSN — the minimum WAL position for replay.
    #[must_use]
    pub const fn recovery_floor_lsn(self) -> Lsn {
        self.durability_boundary().recovery_floor_lsn()
    }

    /// Last completed checkpoint LSN.
    #[must_use]
    pub const fn checkpoint_lsn(self) -> Lsn {
        self.durability_boundary().checkpoint_lsn()
    }

    /// Returns `true` if recovery can start at `lsn`.
    #[must_use]
    pub const fn can_start_recovery_at(self, lsn: Lsn) -> bool {
        self.durability_boundary().can_start_recovery_at(lsn)
    }

    /// Validate identity fields, LSN ordering, and CRC non-zero invariants.
    pub fn validate(&self) -> AndromedaResult<()> {
        self.durability_boundary().validate()
    }

    /// Build the storage format manifest for compatibility gating.
    pub fn storage_format_manifest(&self) -> AndromedaResult<StorageFormatManifest> {
        StorageFormatManifest::from_database_manifest(self)
    }

    // ── Codec ────────────────────────────────────────────────────────────────

    /// Encode this manifest to exactly [`MANIFEST_ENCODED_SIZE`] bytes using
    /// the explicit little-endian format defined in [`crate::codec`].
    ///
    /// The `payload_crc32c` integrity field is computed automatically over the
    /// first 108 bytes and appended at [108..112].
    ///
    /// No native Rust struct layout is used on disk.
    #[must_use]
    pub fn encode(&self) -> [u8; MANIFEST_ENCODED_SIZE] {
        encode_manifest(self)
    }

    /// Decode a manifest from exactly [`MANIFEST_ENCODED_SIZE`] bytes.
    ///
    /// Rejects wrong magic, unsupported format version, and CRC32C mismatch
    /// with a typed [`andromeda_error::AndromedaErrorKind::Storage`] error.
    pub fn decode(bytes: &[u8]) -> AndromedaResult<Self> {
        decode_manifest(bytes)
    }
}
