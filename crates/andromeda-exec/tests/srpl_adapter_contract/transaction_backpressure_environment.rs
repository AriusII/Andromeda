use crate::support::*;

#[test]
fn test_transaction_context_records_updates() {
    let mut tx = SrplTransactionContext::new();
    let obj =
        StructuredObject::new().with_field("status", FieldValue::String("active".to_string()));

    assert!(tx.record_update("users", 0, obj).is_ok());
    assert_eq!(tx.pending_updates().len(), 1);
}
#[test]
fn test_transaction_context_batch_updates() {
    let mut tx = SrplTransactionContext::new();

    for i in 0..10 {
        let obj = StructuredObject::new().with_field("id", FieldValue::Integer(i as i64));
        assert!(tx.record_update("users", i, obj).is_ok());
    }

    assert_eq!(tx.pending_updates().len(), 1); // single table
    assert_eq!(tx.pending_updates()["users"].len(), 10);
}
#[test]
fn test_transaction_context_abort_idempotent() {
    let mut tx = SrplTransactionContext::new();
    tx.abort();
    assert!(tx.is_aborted());

    tx.abort();
    assert!(tx.is_aborted());
}
#[test]
fn test_transaction_context_abort_prevents_updates() {
    let mut tx = SrplTransactionContext::new();
    tx.abort();

    let obj = StructuredObject::new();
    let result = tx.record_update("users", 0, obj);

    assert!(result.is_err());
}
#[test]
fn test_backpressure_respects_limit() {
    let mut bp = SrplStreamBackpressure::new(100);

    assert!(bp.buffer_rows(50).is_ok());
    assert!(bp.buffer_rows(50).is_ok());
    assert!(bp.buffer_rows(1).is_err());
}
#[test]
fn test_backpressure_release() {
    let mut bp = SrplStreamBackpressure::new(100);

    assert!(bp.buffer_rows(80).is_ok());
    bp.release_rows(30);
    assert!(bp.buffer_rows(50).is_ok());
}
#[test]
fn test_backpressure_zero_limit() {
    let mut bp = SrplStreamBackpressure::new(0);

    let result = bp.buffer_rows(1);
    assert!(result.is_err());
}
#[test]
fn test_backpressure_rejects_row_count_overflow() {
    let mut bp = SrplStreamBackpressure::new(usize::MAX);

    bp.buffer_rows(usize::MAX).unwrap();
    let result = bp.buffer_rows(1);

    assert!(result.is_err());
}
#[test]
fn test_backpressure_saturating_release() {
    let mut bp = SrplStreamBackpressure::new(100);

    bp.buffer_rows(50).ok();
    bp.release_rows(100); // Release more than buffered
    assert!(bp.buffer_rows(50).is_ok()); // Should still work
}
#[test]
fn test_environment_input_management() {
    let mut env = SrplTypedEnvironment::new();
    env.add_input("id", FieldValue::Integer(42));
    env.add_input("name", FieldValue::String("test".to_string()));

    assert_eq!(env.get_input_internal("id"), Some(&FieldValue::Integer(42)));
    assert_eq!(
        env.get_input_internal("name"),
        Some(&FieldValue::String("test".to_string()))
    );
    assert_eq!(env.get_input_internal("missing"), None);
}
#[test]
fn test_environment_binding_management() {
    let mut env = SrplTypedEnvironment::new();
    let rows = vec![
        StructuredObject::new().with_field("id", FieldValue::Integer(1)),
        StructuredObject::new().with_field("id", FieldValue::Integer(2)),
    ];

    env.bind_read("users", rows.clone());

    assert_eq!(env.get_binding_internal("users"), Some(&rows));
    assert_eq!(env.get_binding_internal("missing"), None);
}
#[test]
fn test_environment_multiple_bindings() {
    let mut env = SrplTypedEnvironment::new();

    env.bind_read(
        "users",
        vec![StructuredObject::new().with_field("id", FieldValue::Integer(1))],
    );
    env.bind_read(
        "products",
        vec![StructuredObject::new().with_field("sku", FieldValue::String("A123".to_string()))],
    );

    assert!(env.get_binding_internal("users").is_some());
    assert!(env.get_binding_internal("products").is_some());
}
#[test]
fn test_field_value_type_conversions() {
    assert_eq!(FieldValue::Integer(42).as_i64(), Some(42));
    assert_eq!(
        FieldValue::String("hello".to_string()).as_string(),
        Some("hello")
    );
    assert_eq!(FieldValue::Bool(true).as_bool(), Some(true));

    assert_eq!(FieldValue::String("x".to_string()).as_i64(), None);
    assert_eq!(FieldValue::Integer(1).as_string(), None);
}
#[test]
fn test_structured_object_field_operations() {
    let obj = StructuredObject::new()
        .with_field("a", FieldValue::Integer(1))
        .with_field("b", FieldValue::String("test".to_string()))
        .with_field("c", FieldValue::Bool(false));

    assert!(obj.get_field("a").is_some());
    assert!(obj.get_field("b").is_some());
    assert!(obj.get_field("c").is_some());
    assert!(obj.get_field("d").is_none());
}
