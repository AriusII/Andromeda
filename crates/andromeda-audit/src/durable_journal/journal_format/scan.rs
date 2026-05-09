use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::Path,
};

use super::super::{
    DurableAuditFailureKind, DurableAuditRecordIdentity, DurableAuditReplayEvidence,
    DurableAuditReplayQuery, DurableAuditReplayRecord, DurableAuditReplayResult,
    DurableAuditReplayWindow, DurableAuditSinkResult, sink_failure,
};
use super::{
    DurableAuditJournalAppendAnchor, DurableAuditJournalScan, GENESIS_CHAIN_CHECKSUM,
    anchor::{journal_anchor_path, validate_journal_anchor},
    line::parse_journal_line,
};

pub(in crate::durable_journal) fn replay_durable_audit_journal(
    path: &Path,
    query: &DurableAuditReplayQuery,
) -> DurableAuditSinkResult<Vec<DurableAuditReplayRecord>> {
    Ok(
        replay_durable_audit_journal_with_evidence(path, query, DurableAuditReplayWindow::ALL)?
            .records,
    )
}

pub(in crate::durable_journal) fn replay_durable_audit_journal_with_evidence(
    path: &Path,
    query: &DurableAuditReplayQuery,
    window: DurableAuditReplayWindow,
) -> DurableAuditSinkResult<DurableAuditReplayResult> {
    Ok(scan_durable_audit_journal(path, query, window)?.result)
}

fn scan_durable_audit_journal(
    path: &Path,
    query: &DurableAuditReplayQuery,
    window: DurableAuditReplayWindow,
) -> DurableAuditSinkResult<DurableAuditJournalScan> {
    query.validate().map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::ValidationRejected,
            None,
            error.message().to_string(),
        )
    })?;
    window.validate().map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::ValidationRejected,
            None,
            error.message().to_string(),
        )
    })?;

    if !path.exists() {
        let anchor_path = journal_anchor_path(path);
        if anchor_path.exists() {
            return Err(sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                None,
                "durable audit chain anchor exists without durable audit journal",
            ));
        }
        return Ok(DurableAuditJournalScan {
            result: DurableAuditReplayResult {
                evidence: DurableAuditReplayEvidence::empty(query, window),
                records: Vec::new(),
            },
            last_chain_checksum: GENESIS_CHAIN_CHECKSUM,
            first_record_lsn: None,
            last_record_lsn: None,
        });
    }

    validate_journal_tail_is_record_delimited(path)?;
    let chain_anchor_present = journal_anchor_path(path).exists();

    let file = File::open(path).map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            format!("failed to read durable audit journal: {error}"),
        )
    })?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();
    let mut previous_lsn = 0u64;
    let mut records_scanned = 0usize;
    let mut records_matched = 0usize;
    let mut skipped = 0usize;
    let mut first_returned_lsn = None;
    let mut last_returned_lsn = None;
    let mut first_scanned_lsn = None;
    let mut last_scanned_lsn = None;
    let mut previous_chain_checksum = GENESIS_CHAIN_CHECKSUM;

    for (line_index, line) in reader.lines().enumerate() {
        let line = line.map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                None,
                format!("failed to read durable audit journal line: {error}"),
            )
        })?;
        let journal_record =
            parse_journal_line(&line, previous_chain_checksum).map_err(|reason| {
                sink_failure(
                    DurableAuditFailureKind::CorruptionDetected,
                    None,
                    format!(
                        "durable audit journal corruption at line {}: {reason}",
                        line_index + 1
                    ),
                )
            })?;
        let record = journal_record.record;
        records_scanned = records_scanned.saturating_add(1);
        let record_lsn = record.report.evidence.record_lsn;
        first_scanned_lsn.get_or_insert(record_lsn);
        if record_lsn <= previous_lsn {
            return Err(sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                Some(record.report.identity),
                "durable audit journal record LSNs must increase strictly",
            ));
        }
        previous_lsn = record_lsn;
        last_scanned_lsn = Some(record_lsn);
        previous_chain_checksum = journal_record.chain_checksum;

        if record.matches_replay_filter(query) {
            records_matched = records_matched.saturating_add(1);
            if skipped < window.offset {
                skipped = skipped.saturating_add(1);
                continue;
            }
            if records.len() < window.limit {
                let record_lsn = record.report.evidence.record_lsn;
                first_returned_lsn.get_or_insert(record_lsn);
                last_returned_lsn = Some(record_lsn);
                records.push(record);
            }
        }
    }

    let records_returned = records.len();
    let scan = DurableAuditJournalScan {
        result: DurableAuditReplayResult {
            evidence: DurableAuditReplayEvidence {
                records_scanned,
                records_matched,
                records_returned,
                filter_applied: query.has_filter(),
                limit: window.limit,
                offset: window.offset,
                truncated: records_matched.saturating_sub(window.offset) > records_returned,
                first_returned_lsn,
                last_returned_lsn,
                chain_anchor_present,
                first_scanned_lsn,
                last_scanned_lsn,
                tail_chain_checksum: previous_chain_checksum,
            },
            records,
        },
        last_chain_checksum: previous_chain_checksum,
        first_record_lsn: first_scanned_lsn,
        last_record_lsn: last_scanned_lsn,
    };
    validate_journal_anchor(path, &scan)?;
    Ok(scan)
}

pub(in crate::durable_journal) fn next_record_anchor(
    path: &Path,
    identity: Option<DurableAuditRecordIdentity>,
) -> DurableAuditSinkResult<DurableAuditJournalAppendAnchor> {
    let len = fs::metadata(path)
        .map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::WalAppendRejected,
                identity,
                format!("failed to inspect durable audit journal length: {error}"),
            )
        })?
        .len();
    if len == 0 {
        let anchor_path = journal_anchor_path(path);
        if anchor_path.exists() {
            return Err(sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                identity,
                "durable audit chain anchor exists for empty durable audit journal",
            ));
        }
        return Ok(DurableAuditJournalAppendAnchor {
            record_lsn: 1,
            previous_chain_checksum: GENESIS_CHAIN_CHECKSUM,
            first_record_lsn: 1,
            prior_record_count: 0,
        });
    }

    let scan = scan_durable_audit_journal(
        path,
        &DurableAuditReplayQuery::all(),
        DurableAuditReplayWindow::ALL,
    )?;
    scan.result
        .records
        .last()
        .map(|record| DurableAuditJournalAppendAnchor {
            record_lsn: record.report.evidence.record_lsn.saturating_add(1),
            previous_chain_checksum: scan.last_chain_checksum,
            first_record_lsn: scan
                .first_record_lsn
                .unwrap_or(record.report.evidence.record_lsn),
            prior_record_count: scan.result.evidence.records_scanned,
        })
        .ok_or_else(|| {
            sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                identity,
                "durable audit journal contains bytes but no replayable records",
            )
        })
}

fn validate_journal_tail_is_record_delimited(path: &Path) -> DurableAuditSinkResult<()> {
    let len = fs::metadata(path)
        .map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                None,
                format!("failed to inspect durable audit journal length: {error}"),
            )
        })?
        .len();
    if len == 0 {
        return Ok(());
    }

    let mut file = File::open(path).map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            format!("failed to read durable audit journal tail: {error}"),
        )
    })?;
    file.seek(SeekFrom::End(-1)).map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            format!("failed to seek durable audit journal tail: {error}"),
        )
    })?;
    let mut last_byte = [0u8; 1];
    file.read_exact(&mut last_byte).map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            format!("failed to read durable audit journal tail: {error}"),
        )
    })?;
    if last_byte[0] != b'\n' {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit journal has a truncated tail: last record must be record-delimited",
        ));
    }
    Ok(())
}
