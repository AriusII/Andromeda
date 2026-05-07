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
mod journal_mutation;
mod pending_record;
mod policy_requirement;
mod principal_binding;
mod replay_behavior;
mod replay_query;
mod replay_record;
mod retention;
mod sink;
mod sink_report;
mod validation;
mod wal_evidence;

use checksum::checksum64;
pub use decision_gate::{DurableAuditDecisionGate, DurableAuditVisibleDecisionProof};
pub use error::{DurableAuditSinkFailure, DurableAuditSinkResult};
pub(crate) use event_mapping::durable_audit_family;
pub use failure::DurableAuditFailureKind;
pub use family::DurableAuditEventFamily;
pub use file_sink::FileDurableAuditWalSink;
pub use identity::DurableAuditRecordIdentity;
use journal_format::{
    journal_line, journal_payload, next_record_anchor, remove_journal_chain_anchor,
    replay_durable_audit_journal, replay_durable_audit_journal_with_evidence,
    write_journal_chain_anchor,
};
pub use pending_record::PendingDurableAuditRecord;
pub use policy_requirement::{
    DurableAuditPolicyEvidenceRequirement, classify_policy_evidence_requirement,
};
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
use validation::validation_failure;
pub(crate) use validation::{validate_permissioned_critical_policy_binding, validate_record};
pub use wal_evidence::DurableAuditWalEvidence;

use error::sink_failure;
use journal_mutation::{acquire_mutation_lock, journal_sidecar_path, publish_compacted_journal};

pub(crate) fn append_record(
    path: &Path,
    record: PendingDurableAuditRecord,
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

    let _lock = acquire_mutation_lock(path, DurableAuditFailureKind::RetentionRejected, None)?;
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
    let tmp_path = journal_sidecar_path(path, ".compact.tmp");
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
    let mut previous_chain_checksum = 0;
    for record in retained {
        let line = journal_line(record, previous_chain_checksum).map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::RetentionRejected,
                Some(record.report.identity),
                error.message().to_string(),
            )
        })?;
        previous_chain_checksum = line.chain_checksum;
        tmp.write_all(line.text.as_bytes()).map_err(|error| {
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

    publish_compacted_journal(&tmp_path, path)?;
    if let (Some(first), Some(last)) = (retained.first(), retained.last()) {
        write_journal_chain_anchor(
            path,
            None,
            DurableAuditFailureKind::RetentionRejected,
            first.report.evidence.record_lsn,
            last.report.evidence.record_lsn,
            retained.len(),
            previous_chain_checksum,
        )?;
    } else {
        remove_journal_chain_anchor(path, DurableAuditFailureKind::RetentionRejected)?;
    }
    let _ = fs::remove_file(tmp_path);

    Ok(())
}
