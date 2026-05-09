use super::checksum;
use crate::{Lsn, write_ahead_log::WalRecord};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// Typed rejection reason for malformed WAL shipping envelopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShippingSegmentRejection {
    RecordCountMismatch,
    EmptyLsnRange,
    EmptyRecords,
    FirstLsnMismatch,
    LastLsnMismatch,
    ChecksumMismatch,
}

impl ShippingSegmentRejection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RecordCountMismatch => "shipping segment record count mismatch",
            Self::EmptyLsnRange => "shipping segment LSN range invalid",
            Self::EmptyRecords => "shipping segment has no records",
            Self::FirstLsnMismatch => "shipping segment first LSN mismatch",
            Self::LastLsnMismatch => "shipping segment last LSN mismatch",
            Self::ChecksumMismatch => "shipping segment checksum mismatch",
        }
    }

    fn into_error(self) -> AndromedaError {
        AndromedaError::new(AndromedaErrorKind::Storage, self.as_str())
    }
}

/// Segment identity and metadata for shipping protocol.
///
/// This struct describes a single WAL segment as it moves from primary to replicas.
/// It is immutable once created and carries all metadata required for validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShippingSegmentDescriptor {
    /// Unique identifier for this segment (monotonically increasing).
    pub segment_id: u64,
    /// First LSN in this segment (inclusive).
    pub start_lsn: Lsn,
    /// Last LSN in this segment (inclusive).
    pub end_lsn: Lsn,
    /// Number of records in this segment.
    pub record_count: usize,
    /// Checksum of all segment record bytes (FNV-1a 64-bit).
    pub checksum: u64,
}

impl ShippingSegmentDescriptor {
    pub fn new(
        segment_id: u64,
        start_lsn: Lsn,
        end_lsn: Lsn,
        record_count: usize,
        checksum: u64,
    ) -> Self {
        Self {
            segment_id,
            start_lsn,
            end_lsn,
            record_count,
            checksum,
        }
    }

    /// Compute a segment checksum from a slice of WAL records.
    /// Uses FNV-1a folding of record bytes.
    pub fn compute_checksum(records: &[WalRecord]) -> u64 {
        checksum::compute_segment_checksum(records)
    }
}

/// Segment shipping envelope: descriptor + borrowed record bytes.
///
/// This is the atomic unit sent from primary to replica over QUIC.
/// The replica receives this, validates the checksum, and appends records to its local WAL.
#[derive(Debug, Clone, Copy)]
pub struct ShippingSegmentEnvelope<'a> {
    pub descriptor: ShippingSegmentDescriptor,
    pub records: &'a [WalRecord],
}

impl<'a> ShippingSegmentEnvelope<'a> {
    pub fn new(descriptor: ShippingSegmentDescriptor, records: &'a [WalRecord]) -> Self {
        Self {
            descriptor,
            records,
        }
    }

    /// Validate the envelope structure (checksum, record count consistency).
    /// Does NOT validate LSN chain; that is the replica's concern.
    pub fn validate_structure(&self) -> AndromedaResult<()> {
        // Check record count matches.
        if self.descriptor.record_count != self.records.len() {
            return Err(ShippingSegmentRejection::RecordCountMismatch.into_error());
        }

        // Check LSN range is non-empty.
        if self.descriptor.start_lsn > self.descriptor.end_lsn {
            return Err(ShippingSegmentRejection::EmptyLsnRange.into_error());
        }

        // Check records are non-empty.
        if self.records.is_empty() {
            return Err(ShippingSegmentRejection::EmptyRecords.into_error());
        }

        // Check first and last LSN match descriptor.
        let first_record = self
            .records
            .first()
            .ok_or_else(|| ShippingSegmentRejection::EmptyRecords.into_error())?;
        if first_record.header.lsn != self.descriptor.start_lsn {
            return Err(ShippingSegmentRejection::FirstLsnMismatch.into_error());
        }
        let last_record = self
            .records
            .last()
            .ok_or_else(|| ShippingSegmentRejection::EmptyRecords.into_error())?;
        if last_record.header.lsn != self.descriptor.end_lsn {
            return Err(ShippingSegmentRejection::LastLsnMismatch.into_error());
        }

        Ok(())
    }

    /// Validate the segment checksum against the records.
    pub fn validate_checksum(&self) -> AndromedaResult<()> {
        let computed = ShippingSegmentDescriptor::compute_checksum(self.records);
        if computed != self.descriptor.checksum {
            return Err(ShippingSegmentRejection::ChecksumMismatch.into_error());
        }
        Ok(())
    }

    /// Full envelope validation: structure + checksum.
    pub fn validate(&self) -> AndromedaResult<()> {
        self.validate_structure()?;
        self.validate_checksum()?;
        Ok(())
    }
}
