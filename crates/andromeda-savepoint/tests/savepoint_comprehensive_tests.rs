//! Savepoint owner contract tests.
//!
//! This suite intentionally stays inside the `andromeda-savepoint` ownership
//! boundary. It covers savepoint stack behavior, rollback markers, bounded
//! write-set evidence, and the narrow "no WAL/visibility side effects" shape
//! exposed by this crate. Transaction state-machine, WAL replay, MVCC status,
//! and observability tests belong to their owner crates.

#![allow(clippy::too_many_lines)]

use andromeda_error::{AndromedaErrorKind, AndromedaResult};
use andromeda_savepoint::{
    MAX_WRITE_SET_IMAGE_BYTES, MAX_WRITE_SET_OPERATION_KIND_BYTES, MAX_WRITE_SET_RESOURCE_ID_BYTES,
    SavepointId, SavepointRollbackMarker, SavepointStack, TxWriteSet, WriteSetImage,
    WriteSetOperationKind, WriteSetResourceId,
};

fn text_resource(value: &str) -> AndromedaResult<WriteSetResourceId> {
    WriteSetResourceId::try_from_text(value)
}

fn bytes_resource(value: &[u8]) -> AndromedaResult<WriteSetResourceId> {
    WriteSetResourceId::try_from_bytes(value.to_vec())
}

fn image(value: &[u8]) -> AndromedaResult<WriteSetImage> {
    WriteSetImage::try_from_bytes(value.to_vec())
}

fn names(stack: &SavepointStack) -> Vec<&str> {
    stack
        .active()
        .iter()
        .map(|savepoint| savepoint.name.as_str())
        .collect()
}

fn text_resources(write_set: &TxWriteSet) -> Vec<String> {
    write_set
        .entries()
        .iter()
        .filter_map(|entry| match &entry.resource_id {
            WriteSetResourceId::Text(value) => Some(value.clone()),
            WriteSetResourceId::Bytes(_) => None,
        })
        .collect()
}

fn assert_transaction_error<T>(result: AndromedaResult<T>) {
    match result {
        Ok(_) => panic!("expected transaction error"),
        Err(error) => assert_eq!(error.kind(), AndromedaErrorKind::Transaction),
    }
}

#[path = "savepoint_comprehensive_tests/concurrency_isolation.rs"]
mod concurrency_isolation;
#[path = "savepoint_comprehensive_tests/crash_recovery_performance.rs"]
mod crash_recovery_performance;
#[path = "savepoint_comprehensive_tests/error_handling.rs"]
mod error_handling;
#[path = "savepoint_comprehensive_tests/lifecycle.rs"]
mod lifecycle;
#[path = "savepoint_comprehensive_tests/wal_durability.rs"]
mod wal_durability;
