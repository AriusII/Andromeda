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

mod error;
mod identity;
mod image;
mod layout;
mod store;

#[cfg(test)]
mod tests;

pub use andromeda_wal::Lsn;
pub use identity::{AllocationId, ObjectId, PageId};
pub use image::PageImage;
pub use layout::{PageFlags, PageHeader, PageLayoutContract, PageSize, PageTrailer, PageType};
pub use store::{InMemoryPageStore, PageStore};
