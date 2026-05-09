use super::fixtures::new_adapter;

#[test]
fn srpl_execution_adapter_creation() {
    let adapter = new_adapter();

    assert!(!adapter.is_transaction_aborted());
    assert_eq!(adapter.failures().len(), 0);
}

#[test]
fn srpl_execution_adapter_abort_transaction() {
    let mut adapter = new_adapter();

    assert!(!adapter.is_transaction_aborted());
    adapter.abort_transaction();
    assert!(adapter.is_transaction_aborted());
}
