//! Compatibility facade for the page owner crate.
//!
//! Page identifiers, page layout contracts, images, and page-store traits now
//! live in `andromeda-storage-page`. This module preserves historical
//! `andromeda_storage::Page*` imports during the crate extraction.

pub use andromeda_storage_page::{
    AllocationId, InMemoryPageStore, ObjectId, PageFlags, PageHeader, PageId, PageImage,
    PageLayoutContract, PageSize, PageStore, PageTrailer, PageType,
};
