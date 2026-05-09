#![forbid(unsafe_code)]
#![doc = r#"
Boundary crate for Andromeda disk-backed page storage.

This crate owns disk page-store durability contracts that are independent of
storage's page layout and disk-manager implementation. Storage keeps the
current file-backed implementation during the migration and delegates page flush
durability validation here.

C5 invariants:

- Durable page writes must not outrun WAL-before-page-flush policy.
- Page bytes must come from explicit, versioned codecs.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
"#]

use std::fmt::{Display, Formatter};

use andromeda_wal::Lsn;

mod atomic_write;
mod error;
mod extent_map;
mod file;
mod integrity;
mod interface;
mod layout_codec;
mod page_store;

pub use andromeda_segment::{ExtentDescriptor, ExtentId};
pub use andromeda_storage_page::{
    AllocationId, ObjectId, PageFlags, PageHeader, PageId, PageImage, PageLayoutContract, PageSize,
    PageStore, PageTrailer, PageType, integrity_trailer_for_payload, validate_payload_integrity,
};
pub use error::DiskManagerError;
pub use file::FileDiskManager;
pub use interface::DiskManager;
pub use layout_codec::{
    NONE_PAGE_ID, PAGE_SIZE_16K, PAGE_SIZE_32K, PAGE_TRAILER_V0_LEN, PAGE_TYPE_FIXED_ROW,
    PAGE_TYPE_FREE, PAGE_TYPE_HYBRID_ROW, PAGE_TYPE_MANIFEST, PERSISTED_HEADER_LEN,
    PageLayoutCodecError, PersistedPageLayoutV1, decode_optional_page_id,
    encode_page_size_16k_or_32k, optional_page_id_value, page_size_bytes_const,
};
pub use page_store::DiskPageStore;

/// Explicit integrity mode for durable pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageIntegrityMode {
    None,
    HeaderCrc32,
}

/// Error returned when a durable page write boundary is unsafe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageFlushDurabilityError {
    MissingPageLsn,
    WalFenceViolation { page_lsn: u64, durable_lsn: u64 },
}

impl Display for PageFlushDurabilityError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingPageLsn => write!(f, "page flush requires a nonzero page LSN"),
            Self::WalFenceViolation {
                page_lsn,
                durable_lsn,
            } => write!(
                f,
                "WAL-before-page flush violated: page LSN {page_lsn} exceeds durable WAL LSN {durable_lsn}"
            ),
        }
    }
}

impl std::error::Error for PageFlushDurabilityError {}

/// WAL-before-page-flush boundary for disk and page-store callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageFlushDurabilityBoundary {
    pub page_lsn: Lsn,
    pub durable_lsn: Lsn,
}

impl PageFlushDurabilityBoundary {
    pub const fn new(page_lsn: Lsn, durable_lsn: Lsn) -> Self {
        Self {
            page_lsn,
            durable_lsn,
        }
    }

    pub fn validate(self) -> Result<(), PageFlushDurabilityError> {
        if self.durable_lsn < self.page_lsn {
            return Err(PageFlushDurabilityError::WalFenceViolation {
                page_lsn: self.page_lsn.get(),
                durable_lsn: self.durable_lsn.get(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_flush_boundary_requires_wal_through_page_lsn() {
        assert!(
            PageFlushDurabilityBoundary::new(Lsn::new(10), Lsn::new(10))
                .validate()
                .is_ok()
        );
        assert!(
            PageFlushDurabilityBoundary::new(Lsn::new(10), Lsn::new(11))
                .validate()
                .is_ok()
        );

        assert_eq!(
            PageFlushDurabilityBoundary::new(Lsn::new(10), Lsn::new(9))
                .validate()
                .unwrap_err(),
            PageFlushDurabilityError::WalFenceViolation {
                page_lsn: 10,
                durable_lsn: 9
            }
        );
    }

    #[test]
    fn page_flush_boundary_allows_zero_lsn_pages() {
        assert!(
            PageFlushDurabilityBoundary::new(Lsn::new(0), Lsn::new(0))
                .validate()
                .is_ok()
        );
    }
}
