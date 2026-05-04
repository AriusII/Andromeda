use andromeda_core::AndromedaResult;

use crate::{Lsn, WalRecord};

use super::storage_error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalCoverageEvidence {
    pub required_wal_start_lsn: Lsn,
    pub first_replay_record_lsn: Option<Lsn>,
    pub last_durable_lsn: Option<Lsn>,
    pub record_count_in_redo_range: usize,
}

pub(super) fn validate_wal_coverage(
    redo_from_lsn: Lsn,
    durable_records: &[WalRecord],
) -> AndromedaResult<WalCoverageEvidence> {
    if redo_from_lsn.is_zero() {
        return Ok(WalCoverageEvidence {
            required_wal_start_lsn: redo_from_lsn,
            first_replay_record_lsn: durable_records.first().map(|record| record.header.lsn),
            last_durable_lsn: durable_records.last().map(|record| record.header.lsn),
            record_count_in_redo_range: durable_records.len(),
        });
    }

    let mut expected_lsn = None;
    let mut previous_in_redo_range = None;
    let mut saw_redo_start = false;
    let mut first_replay_record_lsn = None;
    let mut last_durable_lsn = None;
    let mut record_count_in_redo_range = 0;

    let mut records_in_redo_range = durable_records
        .iter()
        .filter(|record| record.header.lsn >= redo_from_lsn)
        .peekable();

    while let Some(record) = records_in_redo_range.next() {
        if let Some(expected) = expected_lsn {
            if record.header.lsn < expected {
                return Err(storage_error(
                    "recovery WAL coverage contains duplicate or reordered LSN",
                ));
            }
            if record.header.lsn > expected {
                return Err(storage_error("recovery WAL coverage contains an LSN gap"));
            }
            if record.header.previous_lsn != previous_in_redo_range {
                return Err(storage_error(
                    "recovery WAL coverage previous LSN chain mismatch",
                ));
            }
        } else {
            if record.header.lsn != redo_from_lsn {
                return Err(storage_error(
                    "recovery WAL coverage does not start at required WAL start LSN",
                ));
            }
            if record.header.previous_lsn != expected_previous_lsn_for_recovery_start(redo_from_lsn)
            {
                return Err(storage_error(
                    "recovery WAL coverage start is not anchored to the prior durable LSN",
                ));
            }
            saw_redo_start = true;
            first_replay_record_lsn = Some(record.header.lsn);
        }

        previous_in_redo_range = Some(record.header.lsn);
        last_durable_lsn = Some(record.header.lsn);
        record_count_in_redo_range += 1;
        expected_lsn = if records_in_redo_range.peek().is_some() {
            Some(record.header.lsn.try_next()?)
        } else {
            record.header.lsn.checked_next()
        };
    }

    if !saw_redo_start {
        return Err(storage_error(
            "recovery WAL coverage is missing the required WAL start LSN",
        ));
    }

    Ok(WalCoverageEvidence {
        required_wal_start_lsn: redo_from_lsn,
        first_replay_record_lsn,
        last_durable_lsn,
        record_count_in_redo_range,
    })
}

fn expected_previous_lsn_for_recovery_start(redo_from_lsn: Lsn) -> Option<Lsn> {
    if redo_from_lsn.get() <= 1 {
        None
    } else {
        Some(Lsn::new(redo_from_lsn.get() - 1))
    }
}
