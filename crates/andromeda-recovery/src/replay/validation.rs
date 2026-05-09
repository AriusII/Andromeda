use std::collections::HashMap;

use andromeda_error::AndromedaResult;
use andromeda_wal::{DurableTransactionState, Lsn, WalRecord, WalRecordKind};

use crate::RedoRecordPlan;

use super::{RecoveryReplayTarget, recovery_error};

pub(super) fn build_replay_index<'a, Target: RecoveryReplayTarget>(
    durable_records: &'a [WalRecord],
    target: &mut Target,
) -> AndromedaResult<HashMap<Lsn, &'a WalRecord>> {
    let mut by_lsn: HashMap<Lsn, &WalRecord> = HashMap::with_capacity(durable_records.len());
    for record in durable_records {
        record.validate()?;
        if record.header.kind == WalRecordKind::CheckpointEnd {
            target.observe_checkpoint_end(record.header.lsn);
        }
        if by_lsn.insert(record.header.lsn, record).is_some() {
            return Err(recovery_error(format!(
                "durable WAL replay input contains duplicate LSN {}",
                record.header.lsn.get()
            )));
        }
    }

    Ok(by_lsn)
}

pub(super) fn validate_planned_replay_record(
    redo_rec: &RedoRecordPlan,
    record: &WalRecord,
) -> AndromedaResult<()> {
    if record.header.kind != redo_rec.kind {
        return Err(recovery_error(format!(
            "redo plan LSN {} expects {:?} but durable WAL contains {:?}",
            redo_rec.lsn.get(),
            redo_rec.kind,
            record.header.kind
        )));
    }
    if record.header.transaction_id != redo_rec.transaction_id {
        return Err(recovery_error(format!(
            "redo plan LSN {} transaction evidence does not match durable WAL",
            redo_rec.lsn.get()
        )));
    }
    match (record.header.transaction_id, redo_rec.transaction_state) {
        (Some(_), Some(DurableTransactionState::Committed)) => Ok(()),
        (Some(_), Some(state)) => Err(recovery_error(format!(
            "redo plan attempted to replay non-committed transaction record at LSN {}: state={state:?}",
            redo_rec.lsn.get()
        ))),
        (Some(_), None) => Err(recovery_error(format!(
            "redo plan attempted to replay transaction record at LSN {} without commit evidence",
            redo_rec.lsn.get()
        ))),
        (None, _) => Ok(()),
    }
}
