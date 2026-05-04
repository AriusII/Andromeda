//! Durable catalog WAL payload constants and stable kind-tag mappings.
//!
//! This module owns every compile-time constant that identifies the catalog
//! mutation payload format and the three stable storage WAL kind tags that
//! wrap catalog payloads.  Nothing in this module performs I/O or allocation.

use crate::CatalogMutationRecordKind;

pub(super) const CATALOG_WAL_PAYLOAD_MAGIC: u64 = 0x414e_4452_4341_5457; // "ANDRCATW"
pub(super) const CATALOG_WAL_PAYLOAD_VERSION_V1: u16 = 1;
pub(super) const CATALOG_WAL_PAYLOAD_HEADER_LEN: usize = 28;

/// Stable storage WAL kind tag for `CatalogChangeBegin`.
pub const CATALOG_CHANGE_BEGIN_WAL_KIND_TAG: u16 = 19;
/// Stable storage WAL kind tag for `CatalogChangeApply`.
pub const CATALOG_CHANGE_APPLY_WAL_KIND_TAG: u16 = 20;
/// Stable storage WAL kind tag for `CatalogChangeCommit`.
pub const CATALOG_CHANGE_COMMIT_WAL_KIND_TAG: u16 = 21;

impl CatalogMutationRecordKind {
    /// Stable storage WAL kind tag that must wrap this catalog payload.
    pub const fn storage_wal_kind_tag(self) -> u16 {
        match self {
            Self::CatalogChangeBegin => CATALOG_CHANGE_BEGIN_WAL_KIND_TAG,
            Self::CatalogChangeApply => CATALOG_CHANGE_APPLY_WAL_KIND_TAG,
            Self::CatalogChangeCommit => CATALOG_CHANGE_COMMIT_WAL_KIND_TAG,
        }
    }

    pub(crate) const fn from_storage_wal_kind_tag(tag: u16) -> Option<Self> {
        match tag {
            CATALOG_CHANGE_BEGIN_WAL_KIND_TAG => Some(Self::CatalogChangeBegin),
            CATALOG_CHANGE_APPLY_WAL_KIND_TAG => Some(Self::CatalogChangeApply),
            CATALOG_CHANGE_COMMIT_WAL_KIND_TAG => Some(Self::CatalogChangeCommit),
            _ => None,
        }
    }
}
