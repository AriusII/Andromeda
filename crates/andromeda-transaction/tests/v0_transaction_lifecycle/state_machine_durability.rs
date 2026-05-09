//! State-machine gates that prevent visible terminal status before durable WAL.

use andromeda_core::TransactionId;
use andromeda_transaction::{Lsn, TransactionState, TransactionStateMachine};

#[test]
fn v0_commit_visibility_requires_nonzero_durable_lsn() {
    let mut tx = TransactionStateMachine::new(TransactionId::new(101));

    tx.begin().unwrap();
    tx.request_commit().unwrap();
    assert_eq!(tx.state(), TransactionState::Committing);
    assert!(!tx.is_visible_committed());

    let zero_lsn = tx
        .publish_visible_commit_after_durable_flush(0)
        .unwrap_err();
    assert!(zero_lsn.message().contains("must not be zero"));
    assert_eq!(tx.state(), TransactionState::Committing);
    assert!(!tx.is_visible_committed());

    tx.publish_visible_commit_after_durable_flush(900).unwrap();
    assert_eq!(tx.state(), TransactionState::Committed);
    assert_eq!(tx.durable_commit_lsn(), Some(Lsn::new(900)));
    assert!(tx.is_visible_committed());
}

#[test]
fn v0_rollback_completion_requires_nonzero_durable_lsn() {
    let mut tx = TransactionStateMachine::new(TransactionId::new(102));

    tx.begin().unwrap();
    tx.request_rollback().unwrap();
    assert_eq!(tx.state(), TransactionState::RollingBack);
    assert!(!tx.is_durable_rolled_back());

    let zero_lsn = tx.complete_rollback_after_durable_flush(0).unwrap_err();
    assert!(zero_lsn.message().contains("must not be zero"));
    assert_eq!(tx.state(), TransactionState::RollingBack);
    assert!(!tx.is_durable_rolled_back());

    tx.complete_rollback_after_durable_flush(901).unwrap();
    assert_eq!(tx.state(), TransactionState::RolledBack);
    assert_eq!(tx.durable_rollback_lsn(), Some(Lsn::new(901)));
    assert!(tx.is_durable_rolled_back());
}
