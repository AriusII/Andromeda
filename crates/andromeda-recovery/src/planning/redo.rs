use andromeda_wal::{
    DurableTransactionResume, DurableTransactionState, Lsn, WalRecord, WalRecordKind,
};

use super::RecoveryPlan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedoRecordDecision {
    Replay,
    SkipBeforeRedoStart,
    SkipIncompleteTransaction,
    SkipRolledBackTransaction,
    SkipMissingCommitEvidence,
    SkipNonRedoRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RedoRecordPlan {
    pub lsn: Lsn,
    pub kind: WalRecordKind,
    pub transaction_id: Option<andromeda_types::TransactionId>,
    pub transaction_state: Option<DurableTransactionState>,
    pub decision: RedoRecordDecision,
}

impl RedoRecordPlan {
    pub const fn should_replay(self) -> bool {
        matches!(self.decision, RedoRecordDecision::Replay)
    }
}

pub(super) fn redo_decision_for_record(
    plan: RecoveryPlan,
    record: &WalRecord,
    transaction_evidence: &[DurableTransactionResume],
) -> RedoRecordDecision {
    if record.header.lsn < plan.redo_from_lsn {
        return RedoRecordDecision::SkipBeforeRedoStart;
    }

    if !record.header.kind.is_redo_relevant() {
        return RedoRecordDecision::SkipNonRedoRecord;
    }

    if let Some(transaction_id) = record.header.transaction_id {
        let Some(summary) = transaction_evidence
            .iter()
            .find(|summary| summary.transaction_id == transaction_id)
        else {
            return RedoRecordDecision::SkipMissingCommitEvidence;
        };

        match summary.state {
            DurableTransactionState::Committed => {},
            DurableTransactionState::RolledBack => {
                return RedoRecordDecision::SkipRolledBackTransaction;
            },
            DurableTransactionState::Open | DurableTransactionState::Incomplete => {
                if plan.discard_incomplete_transactions {
                    return RedoRecordDecision::SkipIncompleteTransaction;
                }
                return RedoRecordDecision::SkipMissingCommitEvidence;
            },
        }
    }

    RedoRecordDecision::Replay
}
