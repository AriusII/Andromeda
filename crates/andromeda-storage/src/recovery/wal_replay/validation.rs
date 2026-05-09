use std::collections::HashMap;

use andromeda_error::AndromedaResult;

use crate::{DatabaseManifest, Lsn, WalRecord};

use super::super::storage_error;

pub(super) fn validate_durable_replay_chain(durable_records: &[WalRecord]) -> AndromedaResult<()> {
    let mut previous_lsn = None;
    for record in durable_records {
        record.validate()?;
        let Some(previous) = previous_lsn else {
            previous_lsn = Some(record.header.lsn);
            continue;
        };

        if record.header.lsn <= previous {
            return Err(storage_error(format!(
                "durable WAL replay input must be strictly increasing by LSN: previous={}, current={}",
                previous.get(),
                record.header.lsn.get()
            )));
        }

        if record.header.previous_lsn != Some(previous) {
            let observed = record
                .header
                .previous_lsn
                .map_or_else(|| "None".to_string(), |lsn| lsn.get().to_string());
            return Err(storage_error(format!(
                "durable WAL replay input previous LSN chain mismatch at LSN {}: expected previous {}, observed {}",
                record.header.lsn.get(),
                previous.get(),
                observed
            )));
        }

        previous_lsn = Some(record.header.lsn);
    }

    Ok(())
}

pub(super) fn validate_conflicting_terminal_records(
    durable_records: &[WalRecord],
) -> AndromedaResult<()> {
    let mut terminal_by_transaction = HashMap::new();

    for record in durable_records {
        if !record.is_transaction_terminal() {
            continue;
        }

        let transaction_id = record.header.transaction_id.ok_or_else(|| {
            storage_error("transaction terminal WAL record requires a transaction id")
        })?;
        if let Some((first_kind, first_lsn)) = terminal_by_transaction.insert(
            transaction_id.get(),
            (record.header.kind, record.header.lsn),
        ) {
            return Err(storage_error(format!(
                "durable WAL replay input contains duplicate or conflicting terminal records for transaction {}: first {:?} at LSN {}, second {:?} at LSN {}",
                transaction_id.get(),
                first_kind,
                first_lsn.get(),
                record.header.kind,
                record.header.lsn.get()
            )));
        }
    }

    Ok(())
}

pub(super) fn validate_bootstrap_redo_boundary(
    manifest: &DatabaseManifest,
    durable_records: &[WalRecord],
) -> AndromedaResult<()> {
    if !manifest.required_wal_start_lsn.is_zero() {
        return Ok(());
    }

    if !manifest.base_checkpoint_lsn.is_zero() {
        return Err(storage_error(
            "bootstrap ZERO redo boundary requires a ZERO base checkpoint; \
             non-bootstrap snapshots must name a nonzero required WAL start LSN",
        ));
    }

    if let Some(first) = durable_records.first()
        && (first.header.lsn != Lsn::new(1) || first.header.previous_lsn.is_some())
    {
        return Err(storage_error(
            "bootstrap ZERO redo boundary requires durable WAL to start at LSN 1 \
                 with no previous LSN so recovery cannot skip required WAL",
        ));
    }

    Ok(())
}
