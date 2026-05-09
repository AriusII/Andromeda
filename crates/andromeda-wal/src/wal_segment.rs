//! Canonical WAL segment descriptor and bundled segment value type.
//!
//! [`WalSegmentDescriptor`] and [`WalSegment`] are defined here and only here.
//! The [`crate::write_ahead_log::segment`] module re-exports them as part of
//! the WAL domain facade and must not redefine them.
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{Lsn, WAL_FORMAT_VERSION, WalRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalSegmentDescriptor {
    pub format_version: u16,
    pub segment_id: u64,
    pub first_lsn: Lsn,
    pub last_lsn: Lsn,
    pub base_previous_lsn: Option<Lsn>,
    pub record_count: usize,
}

impl WalSegmentDescriptor {
    pub fn for_records(
        segment_id: u64,
        base_previous_lsn: Option<Lsn>,
        records: &[WalRecord],
    ) -> AndromedaResult<Self> {
        let first = records
            .first()
            .ok_or_else(|| storage_error("WAL segment must contain at least one record"))?;
        let last = records
            .last()
            .ok_or_else(|| storage_error("WAL segment must contain at least one record"))?;
        let descriptor = Self {
            format_version: WAL_FORMAT_VERSION,
            segment_id,
            first_lsn: first.header.lsn,
            last_lsn: last.header.lsn,
            base_previous_lsn,
            record_count: records.len(),
        };
        descriptor.validate_records(records)?;
        Ok(descriptor)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.format_version != WAL_FORMAT_VERSION {
            return Err(storage_error("unsupported WAL segment format version"));
        }
        if self.segment_id == 0 {
            return Err(storage_error("WAL segment id must not be zero"));
        }
        if self.record_count == 0 {
            return Err(storage_error("WAL segment record count must not be zero"));
        }
        if self.first_lsn.is_zero() || self.last_lsn.is_zero() {
            return Err(storage_error("WAL segment LSN bounds must not be zero"));
        }
        if self.last_lsn < self.first_lsn {
            return Err(storage_error("WAL segment last LSN precedes first LSN"));
        }
        if matches!(self.base_previous_lsn, Some(previous) if previous >= self.first_lsn) {
            return Err(storage_error(
                "WAL segment base previous LSN must precede first LSN",
            ));
        }
        match self.base_previous_lsn {
            Some(previous) => {
                if previous.try_next()? != self.first_lsn {
                    return Err(storage_error(
                        "WAL segment base previous LSN must chain to first LSN",
                    ));
                }
            },
            None => {
                // First segment (no base_previous_lsn) can start at any LSN >= 1.
                // This allows restore/PITR scenarios where segments begin at archive start, not global LSN 1.
                if self.first_lsn.is_zero() {
                    return Err(storage_error("WAL segment first LSN must not be zero"));
                }
            },
        }
        Ok(())
    }

    pub fn validate_records(&self, records: &[WalRecord]) -> AndromedaResult<()> {
        self.validate()?;
        if records.len() != self.record_count {
            return Err(storage_error("WAL segment record count mismatch"));
        }
        let Some(first) = records.first() else {
            return Err(storage_error("WAL segment has no records"));
        };
        let Some(last) = records.last() else {
            return Err(storage_error("WAL segment has no records"));
        };
        if first.header.lsn != self.first_lsn || last.header.lsn != self.last_lsn {
            return Err(storage_error("WAL segment LSN bounds mismatch records"));
        }

        let mut expected_lsn = self.first_lsn;
        let mut expected_previous = self.base_previous_lsn;
        for (index, record) in records.iter().enumerate() {
            record.validate()?;
            if record.header.lsn != expected_lsn {
                return Err(storage_error("WAL segment record LSN gap or duplicate"));
            }
            if record.header.previous_lsn != expected_previous {
                return Err(storage_error("WAL segment previous LSN chain mismatch"));
            }
            expected_previous = Some(record.header.lsn);
            if index + 1 < records.len() {
                expected_lsn = record.header.lsn.try_next()?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalSegment {
    pub descriptor: WalSegmentDescriptor,
    pub records: Vec<WalRecord>,
}

impl WalSegment {
    pub fn new(descriptor: WalSegmentDescriptor, records: Vec<WalRecord>) -> AndromedaResult<Self> {
        descriptor.validate_records(&records)?;
        Ok(Self {
            descriptor,
            records,
        })
    }
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WalRecordKind;
    use andromeda_types::TransactionId;

    fn record(lsn: u64, previous_lsn: Option<u64>) -> WalRecord {
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(lsn),
            previous_lsn.map(Lsn::new),
            Some(TransactionId::new(9)),
            b"row".to_vec(),
        )
        .unwrap()
    }

    #[test]
    fn segment_descriptor_validates_gapless_chain() {
        let records = vec![record(1, None), record(2, Some(1))];
        let descriptor = WalSegmentDescriptor::for_records(1, None, &records).unwrap();

        assert_eq!(descriptor.first_lsn, Lsn::new(1));
        assert_eq!(descriptor.last_lsn, Lsn::new(2));
        assert!(WalSegment::new(descriptor, records).is_ok());
    }

    #[test]
    fn segment_descriptor_rejects_previous_lsn_mismatch() {
        let records = vec![record(1, None), record(2, None)];

        assert_eq!(
            WalSegmentDescriptor::for_records(1, None, &records)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
    }
}
