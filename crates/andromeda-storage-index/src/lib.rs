#![forbid(unsafe_code)]
#![doc = r#"
Storage index ownership boundary for Andromeda.

This crate starts the extraction of storage-index-owned identity and
configuration contracts from `andromeda-storage`. Runtime B-Tree mutation and
page-backed node persistence remain in `andromeda-storage` until their WAL,
recovery, and crash-validation contracts are promoted together.
"#]

mod config;
mod identity;

pub use config::{BTREE_DURABLE_FORMAT_PROMOTED, BTreeConfig};
pub use identity::{ColumnId, IndexId, RowId};
