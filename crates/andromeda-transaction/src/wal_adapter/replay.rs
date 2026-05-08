use std::collections::BTreeMap;

use andromeda_core::{AndromedaResult, EngineTimestamp, TransactionId};
use andromeda_transaction_log::{IsolationLevel, Lsn, TxWalReplayRecord};

use super::error::{TxWalAdapterError, tx_adapter_error};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxWalAdapterReplayKind {
    Begin,
    Commit,
    Rollback,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxWalAdapterReplayRecord {
    pub kind: TxWalAdapterReplayKind,
    pub lsn: Lsn,
    pub durable_lsn: Lsn,
    pub tx_id: Option<TransactionId>,
    pub timestamp: EngineTimestamp,
    pub row_count_affected: u64,
    pub isolation_level: IsolationLevel,
    pub parameter_hash: u64,
}

impl TxWalAdapterReplayRecord {
    pub fn begin(tx_id: TransactionId, lsn: Lsn) -> Self {
        Self::boundary(TxWalAdapterReplayKind::Begin, tx_id, lsn)
    }

    pub fn commit(tx_id: TransactionId, lsn: Lsn, timestamp: EngineTimestamp) -> Self {
        Self {
            timestamp,
            ..Self::boundary(TxWalAdapterReplayKind::Commit, tx_id, lsn)
        }
    }

    pub fn rollback(tx_id: TransactionId, lsn: Lsn, timestamp: EngineTimestamp) -> Self {
        Self {
            timestamp,
            ..Self::boundary(TxWalAdapterReplayKind::Rollback, tx_id, lsn)
        }
    }

    pub fn other(lsn: Lsn, tx_id: Option<TransactionId>) -> Self {
        Self {
            kind: TxWalAdapterReplayKind::Other,
            lsn,
            durable_lsn: lsn,
            tx_id,
            timestamp: EngineTimestamp::ZERO,
            row_count_affected: 0,
            isolation_level: IsolationLevel::Snapshot,
            parameter_hash: 0,
        }
    }

    pub fn with_commit_metadata(
        mut self,
        row_count_affected: u64,
        isolation_level: IsolationLevel,
        parameter_hash: u64,
    ) -> Self {
        self.row_count_affected = row_count_affected;
        self.isolation_level = isolation_level;
        self.parameter_hash = parameter_hash;
        self
    }

    pub fn with_parameter_hash(mut self, parameter_hash: u64) -> Self {
        self.parameter_hash = parameter_hash;
        self
    }

    pub fn with_durable_lsn(mut self, durable_lsn: Lsn) -> Self {
        self.durable_lsn = durable_lsn;
        self
    }

    fn boundary(kind: TxWalAdapterReplayKind, tx_id: TransactionId, lsn: Lsn) -> Self {
        Self {
            kind,
            lsn,
            durable_lsn: lsn,
            tx_id: Some(tx_id),
            timestamp: EngineTimestamp::ZERO,
            row_count_affected: 0,
            isolation_level: IsolationLevel::Snapshot,
            parameter_hash: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ReplayState {
    last_lsn: Lsn,
    terminal: Option<TxWalAdapterReplayRecord>,
}

pub fn map_tx_wal_replay_records(
    records: impl IntoIterator<Item = TxWalAdapterReplayRecord>,
) -> AndromedaResult<Vec<TxWalReplayRecord>> {
    let mut states: BTreeMap<TransactionId, ReplayState> = BTreeMap::new();
    let mut previous_record = None;

    for record in records {
        validate_global_replay_lsn_order(&mut previous_record, record)?;
        match record.kind {
            TxWalAdapterReplayKind::Begin => {
                let tx_id = replay_tx_id(record)?;
                if states.contains_key(&tx_id) {
                    return Err(tx_adapter_error(TxWalAdapterError::DuplicateBeginRecord));
                }
                states.insert(
                    tx_id,
                    ReplayState {
                        last_lsn: record.lsn,
                        terminal: None,
                    },
                );
            },
            TxWalAdapterReplayKind::Commit | TxWalAdapterReplayKind::Rollback => {
                let tx_id = replay_tx_id(record)?;
                let state = states
                    .get_mut(&tx_id)
                    .ok_or_else(|| tx_adapter_error(TxWalAdapterError::ReplayRecordWithoutBegin))?;
                validate_replay_lsn_order(state, record.lsn)?;
                validate_terminal_durable_coverage(record)?;
                if let Some(existing) = state.terminal {
                    if existing != record {
                        return Err(tx_adapter_error(
                            TxWalAdapterError::ConflictingTerminalRecord,
                        ));
                    }
                    continue;
                }
                state.terminal = Some(record);
            },
            TxWalAdapterReplayKind::Other => {
                let Some(tx_id) = record.tx_id else {
                    continue;
                };
                let state = states
                    .get_mut(&tx_id)
                    .ok_or_else(|| tx_adapter_error(TxWalAdapterError::ReplayRecordWithoutBegin))?;
                if state.terminal.is_some() {
                    return Err(tx_adapter_error(TxWalAdapterError::RecordAfterTerminal));
                }
                validate_replay_lsn_order(state, record.lsn)?;
            },
        }
    }

    let mut replay_records = Vec::with_capacity(states.len());
    for (tx_id, state) in states {
        replay_records.push(match state.terminal {
            Some(record) if record.kind == TxWalAdapterReplayKind::Commit => {
                TxWalReplayRecord::commit_with_durable_lsn(
                    tx_id,
                    record.lsn,
                    record.durable_lsn,
                    record.timestamp,
                    record.row_count_affected,
                    record.isolation_level,
                )
            },
            Some(record) if record.kind == TxWalAdapterReplayKind::Rollback => {
                TxWalReplayRecord::rollback_with_durable_lsn(
                    tx_id,
                    record.lsn,
                    record.durable_lsn,
                    record.timestamp,
                    record.parameter_hash,
                )
            },
            Some(_) => {
                return Err(tx_adapter_error(TxWalAdapterError::InvariantViolated));
            },
            None => TxWalReplayRecord::incomplete(tx_id, state.last_lsn),
        });
    }

    replay_records.sort_by_key(TxWalReplayRecord::replay_lsn);
    Ok(replay_records)
}

fn replay_tx_id(record: TxWalAdapterReplayRecord) -> AndromedaResult<TransactionId> {
    record
        .tx_id
        .ok_or_else(|| tx_adapter_error(TxWalAdapterError::MissingReplayTransactionId))
}

fn validate_replay_lsn_order(state: &mut ReplayState, lsn: Lsn) -> AndromedaResult<()> {
    if lsn < state.last_lsn {
        return Err(tx_adapter_error(TxWalAdapterError::ReplayLsnRegression));
    }
    state.last_lsn = lsn;
    Ok(())
}

fn validate_terminal_durable_coverage(record: TxWalAdapterReplayRecord) -> AndromedaResult<()> {
    if record.durable_lsn < record.lsn {
        return Err(tx_adapter_error(
            TxWalAdapterError::DurableLsnBehindTerminal,
        ));
    }

    Ok(())
}

fn validate_global_replay_lsn_order(
    previous_record: &mut Option<TxWalAdapterReplayRecord>,
    record: TxWalAdapterReplayRecord,
) -> AndromedaResult<()> {
    if let Some(previous_record) = *previous_record {
        if record.lsn < previous_record.lsn {
            return Err(tx_adapter_error(TxWalAdapterError::ReplayLsnRegression));
        }

        if record.lsn == previous_record.lsn && record != previous_record {
            return Err(tx_adapter_error(TxWalAdapterError::ReplayLsnRegression));
        }
    }

    *previous_record = Some(record);
    Ok(())
}
