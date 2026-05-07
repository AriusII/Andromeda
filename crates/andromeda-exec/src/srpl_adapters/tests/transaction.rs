use super::fixtures::string_row;
use super::*;

#[test]
fn srpl_transaction_context_records_updates() {
    let mut tx = SrplTransactionContext::new();
    let obj = string_row("status", "active");

    assert!(tx.record_update("users", 0, obj).is_ok());
    assert_eq!(tx.pending_updates().len(), 1);
}

#[test]
fn srpl_transaction_context_abort_prevents_updates() {
    let mut tx = SrplTransactionContext::new();
    tx.abort();

    let result = tx.record_update("users", 0, StructuredObject::new());
    assert!(result.is_err());
}

#[test]
fn srpl_backpressure_respects_limits() {
    let mut bp = SrplStreamBackpressure::new(100);

    assert!(bp.buffer_rows(50).is_ok());
    assert!(bp.buffer_rows(50).is_ok());
    assert!(bp.buffer_rows(1).is_err());
}

#[test]
fn srpl_backpressure_release() {
    let mut bp = SrplStreamBackpressure::new(100);

    assert!(bp.buffer_rows(80).is_ok());
    bp.release_rows(30);

    assert!(bp.buffer_rows(50).is_ok());
}
