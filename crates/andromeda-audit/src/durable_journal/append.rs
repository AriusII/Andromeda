use std::{fs::OpenOptions, io::Write, path::Path};

use super::{
    DurableAuditAppendRecord, DurableAuditFailureKind, DurableAuditRecordIdentity,
    DurableAuditReplayRecord, DurableAuditSinkReport, DurableAuditSinkResult,
    DurableAuditWalEvidence,
    checksum::checksum64,
    journal_format::{
        journal_line, journal_payload, next_record_anchor, write_journal_chain_anchor,
    },
    journal_mutation::acquire_mutation_lock,
    sink_failure,
};

pub(crate) fn append_record(
    path: &Path,
    record: DurableAuditAppendRecord,
) -> DurableAuditSinkResult<DurableAuditSinkReport> {
    let identity = record.identity;
    record
        .validate()
        .map_err(|error| validation_failure(identity, error.message().to_string()))?;

    let _lock = acquire_mutation_lock(
        path,
        DurableAuditFailureKind::WalAppendRejected,
        Some(identity),
    )?;
    let anchor = next_record_anchor(path, Some(identity))?;
    let replay_record = replay_record_from_pending(record, anchor.record_lsn)?;
    let line = journal_line(&replay_record, anchor.previous_chain_checksum)
        .map_err(|error| validation_failure(identity, error.message().to_string()))?;
    let record_count = anchor.prior_record_count.checked_add(1).ok_or_else(|| {
        sink_failure(
            DurableAuditFailureKind::WalAppendRejected,
            Some(identity),
            "durable audit journal record count exhausted before append",
        )
    })?;

    write_journal_line(path, identity, &line.text)?;
    write_journal_chain_anchor(
        path,
        Some(identity),
        DurableAuditFailureKind::WalFlushRejected,
        anchor.first_record_lsn,
        anchor.record_lsn,
        record_count,
        line.chain_checksum,
    )?;

    Ok(replay_record.report)
}

fn replay_record_from_pending(
    record: DurableAuditAppendRecord,
    record_lsn: u64,
) -> DurableAuditSinkResult<DurableAuditReplayRecord> {
    let identity = record.identity;
    let mut replay_record = DurableAuditReplayRecord {
        report: DurableAuditSinkReport {
            identity,
            evidence: DurableAuditWalEvidence {
                record_lsn,
                durable_lsn: record_lsn,
                checksum: 1,
            },
            replay_behavior: record.replay_behavior,
            retention: record.retention,
        },
        principal_binding: record.principal_binding,
        event_kind: record.event_kind,
    };

    let payload = journal_payload(&replay_record)
        .map_err(|error| validation_failure(identity, error.message().to_string()))?;
    replay_record.report.evidence.checksum = checksum64(payload.as_bytes());
    replay_record
        .validate()
        .map_err(|error| validation_failure(identity, error.message().to_string()))?;

    Ok(replay_record)
}

fn write_journal_line(
    path: &Path,
    identity: DurableAuditRecordIdentity,
    line: &str,
) -> DurableAuditSinkResult<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::WalAppendRejected,
                Some(identity),
                format!("failed to append durable audit journal record: {error}"),
            )
        })?;
    file.write_all(line.as_bytes()).map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::WalAppendRejected,
            Some(identity),
            format!("failed to write durable audit journal record: {error}"),
        )
    })?;
    file.sync_all().map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::WalFlushRejected,
            Some(identity),
            format!("failed to flush durable audit journal record: {error}"),
        )
    })?;

    Ok(())
}

fn validation_failure(
    identity: DurableAuditRecordIdentity,
    message: impl Into<String>,
) -> super::DurableAuditSinkFailure {
    sink_failure(
        DurableAuditFailureKind::ValidationRejected,
        Some(identity),
        message,
    )
}
