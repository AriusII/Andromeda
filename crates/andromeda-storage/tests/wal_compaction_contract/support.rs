use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use andromeda_storage::write_ahead_log::compaction::{
    CompactionContext, FragmentationMetrics, WalCompactionAuditEvent,
};
use andromeda_storage::{Lsn, WalRecord, WalRecordKind};

pub(crate) struct MockCompactionContext {
    fragmented_segments: Vec<FragmentationMetrics>,
    segment_records: HashMap<u64, Vec<WalRecord>>,
    live_lsns: HashSet<u64>,
    dead_lsns: HashSet<u64>,
    written_segments: Arc<Mutex<Vec<(u64, u64, u64)>>>,
    swapped_segments: Arc<Mutex<Vec<(u64, u64)>>>,
    audit_events: Arc<Mutex<Vec<WalCompactionAuditEvent>>>,
    fail_write: bool,
    fail_swap: bool,
}

impl MockCompactionContext {
    pub(crate) fn new() -> Self {
        MockCompactionContext {
            fragmented_segments: Vec::new(),
            segment_records: HashMap::new(),
            live_lsns: HashSet::new(),
            dead_lsns: HashSet::new(),
            written_segments: Arc::new(Mutex::new(Vec::new())),
            swapped_segments: Arc::new(Mutex::new(Vec::new())),
            audit_events: Arc::new(Mutex::new(Vec::new())),
            fail_write: false,
            fail_swap: false,
        }
    }

    pub(crate) fn with_fragmented_segment(mut self, metrics: FragmentationMetrics) -> Self {
        self.fragmented_segments.push(metrics);
        self
    }

    pub(crate) fn with_segment_records(mut self, segment_id: u64, records: Vec<WalRecord>) -> Self {
        self.segment_records.insert(segment_id, records);
        self
    }

    pub(crate) fn with_live_record(mut self, lsn: u64) -> Self {
        self.live_lsns.insert(lsn);
        self
    }

    pub(crate) fn with_dead_record(mut self, lsn: u64) -> Self {
        self.dead_lsns.insert(lsn);
        self
    }

    pub(crate) fn fail_on_write(mut self) -> Self {
        self.fail_write = true;
        self
    }

    pub(crate) fn fail_on_swap(mut self) -> Self {
        self.fail_swap = true;
        self
    }

    pub(crate) fn get_audit_events(&self) -> Vec<WalCompactionAuditEvent> {
        self.audit_events.lock().unwrap().clone()
    }

    pub(crate) fn get_written_segments(&self) -> Vec<(u64, u64, u64)> {
        self.written_segments.lock().unwrap().clone()
    }

    pub(crate) fn get_swapped_segments(&self) -> Vec<(u64, u64)> {
        self.swapped_segments.lock().unwrap().clone()
    }
}

impl CompactionContext for MockCompactionContext {
    fn identify_fragmented_segments(
        &self,
        threshold_ratio: f64,
    ) -> AndromedaResult<Vec<FragmentationMetrics>> {
        Ok(self
            .fragmented_segments
            .iter()
            .filter(|metrics| metrics.fragmentation_ratio >= threshold_ratio)
            .copied()
            .collect())
    }

    fn read_segment_records(&self, segment_id: u64) -> AndromedaResult<Vec<WalRecord>> {
        self.segment_records
            .get(&segment_id)
            .cloned()
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!("segment {} not found", segment_id),
                )
            })
    }

    fn should_keep_record(&self, record: &WalRecord) -> AndromedaResult<bool> {
        let lsn = record.header.lsn.get();
        if self.live_lsns.contains(&lsn) {
            Ok(true)
        } else if self.dead_lsns.contains(&lsn) {
            Ok(false)
        } else {
            Ok(true)
        }
    }

    fn write_compacted_segment(
        &self,
        original_segment_id: u64,
        records: &[WalRecord],
    ) -> AndromedaResult<(u64, u64)> {
        if self.fail_write {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "simulated write failure",
            ));
        }

        let new_segment_id = original_segment_id + 1000;
        let bytes_written = records
            .iter()
            .map(|record| record.payload.len())
            .sum::<usize>() as u64;

        self.written_segments.lock().unwrap().push((
            original_segment_id,
            new_segment_id,
            bytes_written,
        ));

        Ok((new_segment_id, bytes_written))
    }

    fn swap_segment(&self, old_segment_id: u64, new_segment_id: u64) -> AndromedaResult<()> {
        if self.fail_swap {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "simulated swap failure",
            ));
        }

        self.swapped_segments
            .lock()
            .unwrap()
            .push((old_segment_id, new_segment_id));

        Ok(())
    }

    fn emit_audit_event(&self, event: WalCompactionAuditEvent) -> AndromedaResult<()> {
        self.audit_events.lock().unwrap().push(event);
        Ok(())
    }
}

pub(crate) fn test_record(lsn: u64) -> WalRecord {
    WalRecord::from_parts(
        WalRecordKind::RowInsert,
        Lsn::new(lsn),
        if lsn > 1 {
            Some(Lsn::new(lsn - 1))
        } else {
            None
        },
        Some(TransactionId::new(1)),
        vec![0u8; 64],
    )
    .unwrap()
}
