use super::*;
use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::{LockAcquireStatus, LockManager, LockMode, LockResource};
use andromeda_types::TransactionId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransactionStatus {
    InFlight,
}

#[derive(Debug, Default)]
struct TransactionStatusTable {
    statuses: RefCell<BTreeMap<TransactionId, TransactionStatus>>,
}

impl TransactionStatusTable {
    fn new() -> Self {
        Self::default()
    }

    fn record(&self, tx_id: TransactionId, status: TransactionStatus) -> Result<(), &'static str> {
        if tx_id.get() == 0 {
            return Err("transaction status id must not be zero");
        }

        self.statuses.borrow_mut().insert(tx_id, status);
        Ok(())
    }

    fn status(&self, tx_id: TransactionId) -> Option<TransactionStatus> {
        self.statuses.borrow().get(&tx_id).copied()
    }
}

mod decision;
mod detector;
mod graph;
mod helpers;
mod policy;
