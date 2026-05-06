use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

mod checksum;
mod decision_gate;
mod error;
mod event_mapping;
mod failure;
mod family;
mod file_sink;
mod identity;
mod journal_format;
mod pending_record;
mod principal_binding;
mod replay_behavior;
mod replay_query;
mod replay_record;
mod retention;
mod sink;
mod sink_report;
mod wal_evidence;

use checksum::checksum64;
pub use decision_gate::{DurableAuditDecisionGate, DurableAuditVisibleDecisionProof};
pub use error::{DurableAuditSinkFailure, DurableAuditSinkResult};
pub use event_mapping::durable_audit_family;
pub use failure::DurableAuditFailureKind;
pub use family::DurableAuditEventFamily;
pub use file_sink::FileDurableAuditWalSink;
pub use identity::DurableAuditRecordIdentity;
use journal_format::{
    journal_line, journal_payload, next_record_lsn, replay_durable_audit_journal,
    replay_durable_audit_journal_with_evidence,
};
pub use pending_record::PendingDurableAuditRecord;
pub use principal_binding::DurableAuditPrincipalBinding;
pub use replay_behavior::DurableAuditReplayBehavior;
pub use replay_query::{
    DurableAuditReplayEvidence, DurableAuditReplayLsnRange, DurableAuditReplayQuery,
    DurableAuditReplayResult, DurableAuditReplayWindow,
};
pub use replay_record::DurableAuditReplayRecord;
pub use retention::{
    DurableAuditCompactionReport, DurableAuditPruneBlockReason, DurableAuditPruneEvidence,
    DurableAuditRetentionBoundary, DurableAuditRetentionManager, DurableAuditRetentionPolicy,
    DurableAuditWalSegmentArchiveProof,
};
pub use sink::DurableAuditWalSink;
pub use sink_report::DurableAuditSinkReport;
pub use wal_evidence::DurableAuditWalEvidence;

use error::sink_failure;

use andromeda_core::AndromedaResult;

use super::observe_error;

pub(crate) fn validate_record(record: &PendingDurableAuditRecord) -> AndromedaResult<()> {
    record.identity.validate()?;
    record.principal_binding.validate()?;
    record.envelope.validate()?;

    if record.identity.trace_id != record.envelope.trace_id {
        return Err(observe_error(
            "durable audit identity trace_id must match envelope trace_id",
        ));
    }
    if record.identity.event_id != record.envelope.event_id {
        return Err(observe_error(
            "durable audit identity event_id must match envelope event_id",
        ));
    }

    let Some(family) = durable_audit_family(&record.envelope.event) else {
        return Err(observe_error(
            "durable audit record requires an audit-eligible trace event",
        ));
    };
    if family != record.identity.family {
        return Err(observe_error(
            "durable audit identity family must match envelope event family",
        ));
    }

    if matches!(
        record.identity.family,
        DurableAuditEventFamily::SecurityDecision
    ) && !record.envelope.correlation.has_request_session()
    {
        return Err(observe_error(
            "durable security audit records require request/session correlation",
        ));
    }

    Ok(())
}

pub(crate) fn append_record(
    path: &Path,
    record: PendingDurableAuditRecord,
) -> DurableAuditSinkResult<DurableAuditSinkReport> {
    let identity = record.identity;
    record
        .validate()
        .map_err(|error| validation_failure(identity, error.message().to_string()))?;

    let record_lsn = next_record_lsn(path, Some(identity))?;
    let replay_record = replay_record_from_pending(record, record_lsn)?;
    let line = journal_line(&replay_record)
        .map_err(|error| validation_failure(identity, error.message().to_string()))?;

    write_journal_line(path, identity, &line)?;

    Ok(replay_record.report)
}

fn replay_record_from_pending(
    record: PendingDurableAuditRecord,
    record_lsn: u64,
) -> DurableAuditSinkResult<DurableAuditReplayRecord> {
    let identity = record.identity;
    let event_kind = format!("{:?}", record.envelope.event.kind());
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
        event_kind,
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
) -> DurableAuditSinkFailure {
    sink_failure(
        DurableAuditFailureKind::ValidationRejected,
        Some(identity),
        message,
    )
}

pub(crate) fn open_sink(path: impl AsRef<Path>) -> DurableAuditSinkResult<PathBuf> {
    let path = path.as_ref().to_path_buf();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::WalAppendRejected,
                None,
                format!("failed to create durable audit journal directory: {error}"),
            )
        })?;
    }

    OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(&path)
        .map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::WalAppendRejected,
                None,
                format!("failed to open durable audit journal: {error}"),
            )
        })?;

    replay_durable_audit_journal(&path, &DurableAuditReplayQuery::all())?;
    Ok(path)
}

pub(crate) fn replay_records(
    path: &Path,
    query: &DurableAuditReplayQuery,
) -> DurableAuditSinkResult<Vec<DurableAuditReplayRecord>> {
    replay_durable_audit_journal(path, query)
}

pub(crate) fn replay_records_with_evidence(
    path: &Path,
    query: &DurableAuditReplayQuery,
    window: DurableAuditReplayWindow,
) -> DurableAuditSinkResult<DurableAuditReplayResult> {
    replay_durable_audit_journal_with_evidence(path, query, window)
}

pub(crate) fn compact_records(
    path: &Path,
    policy: &DurableAuditRetentionPolicy,
) -> DurableAuditSinkResult<DurableAuditCompactionReport> {
    compact_records_with_archive_proofs(path, policy, &[])
}

pub(crate) fn compact_records_with_archive_proofs(
    path: &Path,
    policy: &DurableAuditRetentionPolicy,
    archive_proofs: &[DurableAuditWalSegmentArchiveProof],
) -> DurableAuditSinkResult<DurableAuditCompactionReport> {
    policy.validate().map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::RetentionRejected,
            None,
            error.message().to_string(),
        )
    })?;
    for proof in archive_proofs {
        proof.validate().map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::RetentionRejected,
                None,
                error.message().to_string(),
            )
        })?;
    }

    let records = replay_durable_audit_journal(path, &DurableAuditReplayQuery::all())?;
    let records_scanned = records.len();
    let high_water_record_lsn = records
        .last()
        .map(|record| record.report.evidence.record_lsn);
    let manager = DurableAuditRetentionManager::new(policy.clone()).map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::RetentionRejected,
            None,
            error.message().to_string(),
        )
    })?;
    let retained = records
        .into_iter()
        .filter_map(|record| {
            if Some(record.report.evidence.record_lsn) == high_water_record_lsn {
                return Some(Ok(record));
            }
            retain_record_after_prune_evaluation(&manager, archive_proofs, record).transpose()
        })
        .collect::<DurableAuditSinkResult<Vec<_>>>()?;
    let report = DurableAuditCompactionReport::from_records(records_scanned, &retained).map_err(
        |error| {
            sink_failure(
                DurableAuditFailureKind::RetentionRejected,
                None,
                error.message().to_string(),
            )
        },
    )?;

    rewrite_compacted_journal(path, &retained)?;
    replay_durable_audit_journal(path, &DurableAuditReplayQuery::all())?;
    Ok(report)
}

fn retain_record_after_prune_evaluation(
    manager: &DurableAuditRetentionManager,
    archive_proofs: &[DurableAuditWalSegmentArchiveProof],
    record: DurableAuditReplayRecord,
) -> DurableAuditSinkResult<Option<DurableAuditReplayRecord>> {
    let archive_proof = archive_proof_for_record(&record, archive_proofs);
    let evidence = manager
        .evaluate_prune(&record, archive_proof)
        .map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::RetentionRejected,
                Some(record.report.identity),
                error.message().to_string(),
            )
        })?;
    Ok((!evidence.prune_allowed).then_some(record))
}

fn archive_proof_for_record(
    record: &DurableAuditReplayRecord,
    archive_proofs: &[DurableAuditWalSegmentArchiveProof],
) -> Option<DurableAuditWalSegmentArchiveProof> {
    archive_proofs
        .iter()
        .find(|proof| {
            proof.covers(record.report.evidence)
                && proof.checksum == record.report.evidence.checksum
        })
        .or_else(|| {
            archive_proofs
                .iter()
                .find(|proof| proof.covers(record.report.evidence))
        })
        .cloned()
}

fn rewrite_compacted_journal(
    path: &Path,
    retained: &[DurableAuditReplayRecord],
) -> DurableAuditSinkResult<()> {
    let tmp_path = path.with_extension(format!(
        "{}.compact.tmp",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("audit")
    ));
    let _ = fs::remove_file(&tmp_path);

    let mut tmp = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
        .map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::RetentionRejected,
                None,
                format!("failed to open durable audit compaction temporary journal: {error}"),
            )
        })?;
    for record in retained {
        let line = journal_line(record).map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::RetentionRejected,
                Some(record.report.identity),
                error.message().to_string(),
            )
        })?;
        tmp.write_all(line.as_bytes()).map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::RetentionRejected,
                Some(record.report.identity),
                format!("failed to write durable audit compaction temporary journal: {error}"),
            )
        })?;
    }
    tmp.sync_all().map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::RetentionRejected,
            None,
            format!("failed to flush durable audit compaction temporary journal: {error}"),
        )
    })?;
    drop(tmp);

    let compacted = fs::read(&tmp_path).map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::RetentionRejected,
            None,
            format!("failed to read durable audit compaction temporary journal: {error}"),
        )
    })?;
    let mut target = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::RetentionRejected,
                None,
                format!("failed to rewrite durable audit compacted journal: {error}"),
            )
        })?;
    target.write_all(&compacted).map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::RetentionRejected,
            None,
            format!("failed to persist durable audit compacted journal: {error}"),
        )
    })?;
    target.sync_all().map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::RetentionRejected,
            None,
            format!("failed to flush durable audit compacted journal: {error}"),
        )
    })?;
    drop(target);
    let _ = fs::remove_file(tmp_path);

    Ok(())
}
