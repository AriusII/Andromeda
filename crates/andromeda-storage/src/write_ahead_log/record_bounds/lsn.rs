use crate::{Lsn, WalRecord};
use andromeda_core::AndromedaResult;

use super::errors::storage_error;

/// Validate LSN monotonicity and continuity in a record sequence.
///
/// The sequence must form an exact durable chain: each record LSN is the next
/// value after the expected previous LSN, and each `previous_lsn` links to that
/// expected value.
pub fn validate_lsn_continuity(
    base_previous_lsn: Option<Lsn>,
    records: &[WalRecord],
) -> AndromedaResult<Lsn> {
    if records.is_empty() {
        return Err(storage_error(
            "cannot validate LSN continuity for empty record list",
        ));
    }

    let mut expected_previous = base_previous_lsn;
    let mut last_lsn = records[0].header.lsn;

    for (index, record) in records.iter().enumerate() {
        let expected_lsn = match expected_previous {
            Some(previous) => previous.try_next()?,
            None => Lsn::new(1),
        };

        if record.header.lsn != expected_lsn {
            return Err(storage_error(format!(
                "record {} LSN mismatch: expected {}, got {}",
                index,
                expected_lsn.get(),
                record.header.lsn.get()
            )));
        }

        if record.header.previous_lsn != expected_previous {
            return Err(storage_error(format!(
                "record {} previous_lsn mismatch: expected {:?}, got {:?}",
                index, expected_previous, record.header.previous_lsn
            )));
        }

        last_lsn = record.header.lsn;
        expected_previous = Some(record.header.lsn);
    }

    Ok(last_lsn)
}
