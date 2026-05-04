//! Durable catalog mutation record payload codec.

mod codec;
mod constants;

pub use constants::{
    CATALOG_CHANGE_APPLY_WAL_KIND_TAG, CATALOG_CHANGE_BEGIN_WAL_KIND_TAG,
    CATALOG_CHANGE_COMMIT_WAL_KIND_TAG,
};
