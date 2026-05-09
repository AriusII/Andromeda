//! Storage page layout compatibility surface.
//!
//! Only `layout::page` remains because an active page ownership invariant test
//! still verifies this historical import path. Other layout-domain callers
//! should import storage root reexports or owner crates directly.

pub mod page;
