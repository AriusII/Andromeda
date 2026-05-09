use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::TransactionId;

use crate::Lsn;

use super::{WalRecord, WalRecordKind, validate_wal_record_bounds};

pub(crate) fn try_next_lsn(tail_lsn: Option<Lsn>) -> AndromedaResult<Lsn> {
    match tail_lsn {
        Some(last_lsn) => last_lsn.try_next(),
        None => Ok(Lsn::new(1)),
    }
}

pub(crate) fn next_lsn_or_zero(tail_lsn: Option<Lsn>) -> Lsn {
    try_next_lsn(tail_lsn).unwrap_or(Lsn::ZERO)
}

pub(crate) fn payload_record(
    kind: WalRecordKind,
    tail_lsn: Option<Lsn>,
    transaction_id: Option<TransactionId>,
    payload: impl Into<Vec<u8>>,
) -> AndromedaResult<WalRecord> {
    WalRecord::from_parts(
        kind,
        try_next_lsn(tail_lsn)?,
        tail_lsn,
        transaction_id,
        payload,
    )
}

pub(crate) fn validate_append_record(
    record: &WalRecord,
    tail_lsn: Option<Lsn>,
    lsn_mismatch_msg: &'static str,
    previous_lsn_mismatch_msg: &'static str,
) -> AndromedaResult<()> {
    record.validate()?;
    validate_wal_record_bounds(record)?;

    let expected_lsn = try_next_lsn(tail_lsn)?;
    if record.header.lsn != expected_lsn {
        return Err(storage_error(lsn_mismatch_msg));
    }
    if record.header.previous_lsn != tail_lsn {
        return Err(storage_error(previous_lsn_mismatch_msg));
    }

    Ok(())
}

pub(crate) fn validate_flush_target(
    requested_lsn: Lsn,
    durable_lsn: Lsn,
    tail_lsn: Option<Lsn>,
    empty_log_msg: &'static str,
    beyond_tail_msg: &'static str,
) -> AndromedaResult<Option<Lsn>> {
    if requested_lsn.is_zero() || requested_lsn <= durable_lsn {
        return Ok(None);
    }

    let Some(last_lsn) = tail_lsn else {
        return Err(storage_error(empty_log_msg));
    };
    if requested_lsn > last_lsn {
        return Err(storage_error(beyond_tail_msg));
    }

    Ok(Some(requested_lsn))
}

pub(crate) fn durable_records(
    records: &[WalRecord],
    durable_lsn: Lsn,
) -> impl Iterator<Item = &WalRecord> {
    records
        .iter()
        .filter(move |record| record.header.lsn <= durable_lsn)
}

pub(crate) fn replay_durable(records: &[WalRecord], durable_lsn: Lsn) -> Vec<WalRecord> {
    durable_records(records, durable_lsn).cloned().collect()
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
