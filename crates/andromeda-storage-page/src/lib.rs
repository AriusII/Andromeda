#![forbid(unsafe_code)]
#![doc = r#"
C5 owner crate for Andromeda durable page-format boundaries.

This crate owns page identifiers, page layout contracts, full-page byte images,
and the deterministic page-store abstraction used by storage tests. Concrete
disk IO, buffer-pool residency, manifest publication, and WAL file ownership
remain outside this crate.

C5 invariants:

- A page flush must not outrun durable WAL coverage through the page LSN.
- Page bytes must be versioned and explicitly encoded.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
"#]

use std::fmt::{Display, Formatter};

mod btree_node_format_v1;
mod error;
mod identity;
mod image;
mod layout;
mod store;

#[cfg(test)]
mod tests;

pub use andromeda_wal::Lsn;
pub use btree_node_format_v1::{
    BTREE_NODE_V1_FORMAT_VERSION, BTREE_NODE_V1_HEADER_LEN, BTREE_NODE_V1_MAGIC, BTreeNodeHeaderV1,
    BTreeNodeKindV1, BTreeNodeV1,
};
pub use identity::{AllocationId, ObjectId, PageId};
pub use image::PageImage;
pub use layout::{PageFlags, PageHeader, PageLayoutContract, PageSize, PageTrailer, PageType};
pub use store::{InMemoryPageStore, PageStore};

/// Error returned when a dirty page is flushed before its WAL is durable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageFlushDurabilityError {
    MissingPageLsn,
    WalFenceViolation { page_lsn: u64, durable_lsn: u64 },
}

impl Display for PageFlushDurabilityError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingPageLsn => f.write_str("page flush requires a nonzero page LSN"),
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

/// WAL-before-page-flush boundary for page-store callers.
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
        if self.page_lsn.is_zero() {
            return Ok(());
        }
        if self.durable_lsn < self.page_lsn {
            return Err(PageFlushDurabilityError::WalFenceViolation {
                page_lsn: self.page_lsn.get(),
                durable_lsn: self.durable_lsn.get(),
            });
        }
        Ok(())
    }
}

/// Validate that a dirty page may be flushed only after its WAL LSN is durable.
pub fn validate_wal_durability_before_page_flush(
    page_lsn: Lsn,
    durable_lsn: Lsn,
) -> Result<(), PageFlushDurabilityError> {
    PageFlushDurabilityBoundary::new(page_lsn, durable_lsn).validate()
}
