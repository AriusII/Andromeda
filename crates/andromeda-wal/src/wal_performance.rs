use std::time::Duration;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{Lsn, WalRecord, encode_wal_record};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalIoQueueClass {
    P0Durability,
    P1Maintenance,
}

impl WalIoQueueClass {
    pub const fn is_commit_critical(self) -> bool {
        matches!(self, Self::P0Durability)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalQueueSeparationEvidence {
    pub queue_class: WalIoQueueClass,
    pub shares_queue_with_temp_spill: bool,
}

impl WalQueueSeparationEvidence {
    pub const fn new(queue_class: WalIoQueueClass, shares_queue_with_temp_spill: bool) -> Self {
        Self {
            queue_class,
            shares_queue_with_temp_spill,
        }
    }

    pub const fn p0_durable() -> Self {
        Self::new(WalIoQueueClass::P0Durability, false)
    }

    pub const fn preserves_p0_separation(self) -> bool {
        !self.queue_class.is_commit_critical() || !self.shares_queue_with_temp_spill
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if !self.preserves_p0_separation() {
            return Err(storage_error(
                "commit-critical WAL P0 queue must remain separated from temp/spill queues",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalQueueDepthMetrics {
    pub separation: WalQueueSeparationEvidence,
    pub queued_records: u64,
    pub queued_bytes: u64,
}

impl WalQueueDepthMetrics {
    pub fn new(
        separation: WalQueueSeparationEvidence,
        queued_records: u64,
        queued_bytes: u64,
    ) -> AndromedaResult<Self> {
        let metrics = Self {
            separation,
            queued_records,
            queued_bytes,
        };
        metrics.validate()?;
        Ok(metrics)
    }

    pub fn validate(self) -> AndromedaResult<()> {
        self.separation.validate()?;
        if (self.queued_records == 0) != (self.queued_bytes == 0) {
            return Err(storage_error(
                "WAL queue depth must report records and bytes together",
            ));
        }
        Ok(())
    }

    pub const fn has_backlog(self) -> bool {
        self.queued_records != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalWriteAmplificationMetrics {
    pub logical_bytes: u64,
    pub physical_bytes: u64,
}

impl WalWriteAmplificationMetrics {
    pub fn new(logical_bytes: u64, physical_bytes: u64) -> AndromedaResult<Self> {
        let metrics = Self {
            logical_bytes,
            physical_bytes,
        };
        metrics.validate()?;
        Ok(metrics)
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if self.logical_bytes == 0 || self.physical_bytes == 0 {
            return Err(storage_error(
                "WAL write amplification requires non-zero logical and physical byte counts",
            ));
        }
        if self.physical_bytes < self.logical_bytes {
            return Err(storage_error(
                "WAL physical bytes must be greater than or equal to logical bytes",
            ));
        }
        Ok(())
    }

    pub fn ratio(self) -> f64 {
        self.physical_bytes as f64 / self.logical_bytes as f64
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalFlushTelemetry {
    pub separation: WalQueueSeparationEvidence,
    pub target_lsn: Lsn,
    pub queue_depth_before: WalQueueDepthMetrics,
    pub flushed_records: u64,
    pub flush_latency: Duration,
    pub logical_bytes: u64,
    pub physical_bytes: u64,
}

impl WalFlushTelemetry {
    #[allow(
        clippy::too_many_arguments,
        reason = "Flush telemetry keeps the validated WAL evidence explicit."
    )]
    pub fn new(
        separation: WalQueueSeparationEvidence,
        target_lsn: Lsn,
        queue_depth_before: WalQueueDepthMetrics,
        flushed_records: u64,
        flush_latency: Duration,
        logical_bytes: u64,
        physical_bytes: u64,
    ) -> AndromedaResult<Self> {
        let telemetry = Self {
            separation,
            target_lsn,
            queue_depth_before,
            flushed_records,
            flush_latency,
            logical_bytes,
            physical_bytes,
        };
        telemetry.validate()?;
        Ok(telemetry)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.separation.validate()?;
        self.queue_depth_before.validate()?;
        if self.target_lsn.is_zero() {
            return Err(storage_error(
                "WAL flush telemetry requires a non-zero flush target LSN",
            ));
        }
        if self.flush_latency.is_zero() {
            return Err(storage_error(
                "WAL flush telemetry requires a non-zero flush latency sample",
            ));
        }
        if self.flushed_records == 0 {
            return Err(storage_error(
                "WAL flush telemetry requires at least one flushed record",
            ));
        }
        if self.queue_depth_before.queued_records < self.flushed_records {
            return Err(storage_error(
                "WAL flush telemetry cannot flush more records than were queued",
            ));
        }
        if self.queue_depth_before.queued_bytes < self.physical_bytes {
            return Err(storage_error(
                "WAL flush telemetry cannot flush more bytes than were queued",
            ));
        }
        self.write_amplification()?.validate()?;
        Ok(())
    }

    pub fn write_amplification(&self) -> AndromedaResult<WalWriteAmplificationMetrics> {
        WalWriteAmplificationMetrics::new(self.logical_bytes, self.physical_bytes)
    }
}

pub(crate) fn wal_queue_depth_from_records(
    records: &[WalRecord],
    durable_lsn: Lsn,
    separation: WalQueueSeparationEvidence,
) -> AndromedaResult<WalQueueDepthMetrics> {
    let pending = pending_records(records, durable_lsn);
    let queued_records = pending.len() as u64;
    let queued_bytes = encoded_physical_bytes(&pending)?;
    WalQueueDepthMetrics::new(separation, queued_records, queued_bytes)
}

pub(crate) fn wal_flush_telemetry_from_records(
    records: &[WalRecord],
    durable_lsn: Lsn,
    target_lsn: Lsn,
    separation: WalQueueSeparationEvidence,
    flush_latency: Duration,
) -> AndromedaResult<WalFlushTelemetry> {
    if target_lsn <= durable_lsn {
        return Err(storage_error(
            "WAL flush telemetry target must advance beyond the current durable LSN",
        ));
    }

    let queue_depth_before = wal_queue_depth_from_records(records, durable_lsn, separation)?;
    let pending = pending_records(records, durable_lsn);
    let mut flushed_records = 0u64;
    let mut logical_bytes = 0u64;
    let mut physical_bytes = 0u64;
    let mut found_target = false;

    for record in pending {
        flushed_records = flushed_records.saturating_add(1);
        logical_bytes = logical_bytes.saturating_add(record.payload.len() as u64);
        physical_bytes = physical_bytes.saturating_add(encoded_record_len(&record)?);

        if record.header.lsn == target_lsn {
            found_target = true;
            break;
        }
    }

    if !found_target {
        return Err(storage_error(
            "WAL flush telemetry target LSN was not appended",
        ));
    }

    WalFlushTelemetry::new(
        separation,
        target_lsn,
        queue_depth_before,
        flushed_records,
        flush_latency,
        logical_bytes,
        physical_bytes,
    )
}

fn pending_records(records: &[WalRecord], durable_lsn: Lsn) -> Vec<WalRecord> {
    records
        .iter()
        .filter(|record| record.header.lsn > durable_lsn)
        .cloned()
        .collect()
}

fn encoded_physical_bytes(records: &[WalRecord]) -> AndromedaResult<u64> {
    records.iter().try_fold(0u64, |total, record| {
        total
            .checked_add(encoded_record_len(record)?)
            .ok_or_else(|| storage_error("WAL byte count would overflow u64"))
    })
}

fn encoded_record_len(record: &WalRecord) -> AndromedaResult<u64> {
    u64::try_from(encode_wal_record(record)?.len())
        .map_err(|_| storage_error("WAL encoded record length does not fit u64"))
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InMemoryWal, WalRecordKind};

    #[test]
    fn p0_queue_separation_rejects_shared_temp_spill_lane() {
        let error = WalQueueSeparationEvidence::new(WalIoQueueClass::P0Durability, true)
            .validate()
            .unwrap_err();
        assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    }

    #[test]
    fn in_memory_wal_queue_depth_tracks_pending_records() {
        let mut wal = InMemoryWal::new();
        wal.append_payload(WalRecordKind::PageFormat, None, vec![1, 2, 3])
            .expect("append first record");
        let second_lsn = wal
            .append_payload(WalRecordKind::PageFormat, None, vec![4, 5, 6, 7])
            .expect("append second record");

        wal.flush_through(Lsn::new(1)).expect("flush first record");

        let queue_depth = wal
            .queue_depth_metrics(WalQueueSeparationEvidence::p0_durable())
            .expect("pending WAL queue depth metrics");

        assert_eq!(queue_depth.queued_records, 1);
        assert!(queue_depth.queued_bytes > 0);
        assert!(queue_depth.has_backlog());
        assert_eq!(second_lsn, Lsn::new(2));
    }

    #[test]
    fn flush_telemetry_reports_write_amplification_and_latency() {
        let mut wal = InMemoryWal::new();
        wal.append_payload(WalRecordKind::PageFormat, None, vec![1, 2, 3, 4])
            .expect("append first record");
        let target_lsn = wal
            .append_payload(WalRecordKind::PageFormat, None, vec![5, 6, 7, 8, 9])
            .expect("append second record");

        let telemetry = wal
            .flush_through_with_metrics(
                target_lsn,
                WalQueueSeparationEvidence::p0_durable(),
                Duration::from_micros(250),
            )
            .expect("flush telemetry for in-memory WAL");

        assert_eq!(telemetry.target_lsn, target_lsn);
        assert_eq!(telemetry.flushed_records, 2);
        assert_eq!(telemetry.queue_depth_before.queued_records, 2);
        assert_eq!(telemetry.flush_latency, Duration::from_micros(250));
        assert!(telemetry.physical_bytes >= telemetry.logical_bytes);
        assert!(telemetry.write_amplification().unwrap().ratio() >= 1.0);
    }
}
