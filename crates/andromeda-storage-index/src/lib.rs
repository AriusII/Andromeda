#![forbid(unsafe_code)]
#![doc = r#"
Storage index ownership boundary for Andromeda.

This crate starts the extraction of storage-index-owned identity and
configuration contracts from `andromeda-storage`. Runtime B-Tree mutation and
page-backed node persistence remain in `andromeda-storage` until their WAL,
recovery, and crash-validation contracts are promoted together.
"#]

mod config;
mod engine;
mod format_validation;
mod identity;
mod key_codec;
mod key_comparator;

pub use config::{BTREE_DURABLE_FORMAT_PROMOTED, BTreeConfig};
pub use engine::{
    BTreeFormatIdentityError, BTreeKeyFormatIdentity, BTreeOperationType, BTreeStatistics,
    KeyValuePair, validate_btree_key_format_identity_parts,
};
pub use format_validation::KeyV1FormatGate;
pub use identity::{ColumnId, IndexId, RowId};
pub use key_codec::{Key, KeyCodec, KeyDatum, KeyScalarType};
pub use key_comparator::KeyComparator;
