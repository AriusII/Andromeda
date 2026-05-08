#![forbid(unsafe_code)]
#![doc = r#"
Andromeda savepoint stacks and partial rollback metadata.

This crate owns storage-agnostic savepoint identifiers, nesting rules, bounded
write-set evidence, and transaction-local rollback coordination. Savepoint
operations never publish transaction visibility and never replace durable
terminal WAL evidence.

C5 invariants:

- Savepoint rollback must not publish commit visibility before durable WAL evidence exists.
- Durable savepoint evidence must be bounded, versioned, and explicitly encoded.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
"#]

mod stack;
mod write_set;

pub use stack::{
    Savepoint, SavepointId, SavepointReleaseEvidence, SavepointRollbackEvidence,
    SavepointRollbackMarker, SavepointStack,
};
pub use write_set::{
    MAX_WRITE_SET_IMAGE_BYTES, MAX_WRITE_SET_OPERATION_KIND_BYTES, MAX_WRITE_SET_RESOURCE_ID_BYTES,
    TxWriteSet, WriteSetEntry, WriteSetImage, WriteSetOperationKind, WriteSetOrdinal,
    WriteSetResourceId,
};
