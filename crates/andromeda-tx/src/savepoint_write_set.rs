//! Compatibility re-exports for savepoint write-set state.
//!
//! The implementation lives in `andromeda-savepoint`; `andromeda-tx` keeps this
//! module as the legacy import path while transaction extraction continues.

pub use andromeda_savepoint::{
    MAX_WRITE_SET_IMAGE_BYTES, MAX_WRITE_SET_OPERATION_KIND_BYTES, MAX_WRITE_SET_RESOURCE_ID_BYTES,
    TxWriteSet, WriteSetEntry, WriteSetImage, WriteSetOperationKind, WriteSetOrdinal,
    WriteSetResourceId,
};
