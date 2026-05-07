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
mod policy_requirement;
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
pub use wal_evidence::DurableAuditWalEvidence;

use error::sink_failure;

use andromeda_core::AndromedaResult;

use super::{TraceEvent, observe_error};

struct DurableAuditMutationLock {
    path: PathBuf,
}

impl Drop for DurableAuditMutationLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

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

    if matches!(family, DurableAuditEventFamily::SecurityDecision) {
        validate_security_decision_binding(record)?;
    }
    validate_permissioned_critical_policy_binding(
        family,
        &record.principal_binding,
        "durable audit records",
    )?;

    Ok(())
}

pub(crate) fn validate_permissioned_critical_policy_binding(
    family: DurableAuditEventFamily,
    binding: &DurableAuditPrincipalBinding,
    context: &str,
) -> AndromedaResult<()> {
    if !classify_policy_evidence_requirement(family, binding).requires_policy_evidence() {
        return Ok(());
    }

    let Some(policy_version) = binding.policy_version.as_ref() else {
        return Err(observe_error(format!(
            "{context} require policy version evidence for permissioned critical {family:?} records",
        )));
    };
    if !policy_version.has_version_evidence() {
        return Err(observe_error(format!(
            "{context} require non-zero canonical policy version and digest evidence for permissioned critical {family:?} records",
        )));
    }

    Ok(())
}

fn validate_security_decision_binding(record: &PendingDurableAuditRecord) -> AndromedaResult<()> {
    if !record.envelope.correlation.has_request_session() {
        return Err(observe_error(
            "durable security audit records require request/session correlation",
        ));
    }

    let binding = &record.principal_binding;
    if binding.certificate_fingerprint.is_none()
        || binding.surface.is_none()
        || binding.permission.is_none()
        || binding.policy_version.is_none()
    {
        return Err(observe_error(
            "durable security audit records require certificate, surface, permission, and policy version evidence",
        ));
    }
    if binding.request_id != record.envelope.correlation.request_id
        || binding.session_id != record.envelope.correlation.session_id
    {
        return Err(observe_error(
            "durable security audit principal binding request/session ids must match envelope correlation",
        ));
    }

    if let TraceEvent::SecurityAudit(trace) = &record.envelope.event {
        if binding.principal_id != trace.principal.principal_id {
            return Err(observe_error(
                "durable security audit principal_id must match security audit trace",
            ));
        }
        if binding.certificate_fingerprint.as_deref()
            != Some(trace.certificate.fingerprint.as_str())
        {
            return Err(observe_error(
                "durable security audit certificate fingerprint must match security audit trace",
            ));
        }
        if binding.surface != Some(trace.surface) {
            return Err(observe_error(
                "durable security audit surface must match security audit trace",
            ));
        }
        if binding.permission != Some(trace.permission) {
            return Err(observe_error(
                "durable security audit permission must match security audit trace",
            ));
        }
        if binding.policy_version.as_ref() != Some(&trace.policy_version) {
            return Err(observe_error(
                "durable security audit policy version evidence must match security audit trace",
            ));
        }
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

fn acquire_mutation_lock(
    path: &Path,
    failure_kind: DurableAuditFailureKind,
    identity: Option<DurableAuditRecordIdentity>,
) -> DurableAuditSinkResult<DurableAuditMutationLock> {
    let lock_path = journal_sidecar_path(path, ".lock");
    let mut lock = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|error| {
            let reason = if error.kind() == std::io::ErrorKind::AlreadyExists {
                "durable audit journal mutation lock is already held".to_string()
            } else {
                format!("failed to acquire durable audit journal mutation lock: {error}")
            };
            sink_failure(failure_kind, identity, reason)
        })?;
    lock.write_all(b"andromeda-durable-audit-mutation-lock-v1\n")
        .map_err(|error| {
            let _ = fs::remove_file(&lock_path);
            sink_failure(
                failure_kind,
                identity,
                format!("failed to write durable audit journal mutation lock: {error}"),
            )
        })?;
    lock.sync_all().map_err(|error| {
        let _ = fs::remove_file(&lock_path);
        sink_failure(
            failure_kind,
            identity,
            format!("failed to flush durable audit journal mutation lock: {error}"),
        )
    })?;
    Ok(DurableAuditMutationLock { path: lock_path })
}

fn publish_compacted_journal(tmp_path: &Path, path: &Path) -> DurableAuditSinkResult<()> {
    match fs::rename(tmp_path, path) {
        Ok(()) => Ok(()),
        Err(error) if path.exists() => {
            fs::remove_file(path).map_err(|remove_error| {
                sink_failure(
                    DurableAuditFailureKind::RetentionRejected,
                    None,
                    format!(
                        "failed to replace durable audit journal during compaction after rename error {error}: {remove_error}"
                    ),
                )
            })?;
            fs::rename(tmp_path, path).map_err(|rename_error| {
                sink_failure(
                    DurableAuditFailureKind::RetentionRejected,
                    None,
                    format!("failed to publish durable audit compacted journal: {rename_error}"),
                )
            })
        }
        Err(error) => Err(sink_failure(
            DurableAuditFailureKind::RetentionRejected,
            None,
            format!("failed to publish durable audit compacted journal: {error}"),
        )),
    }?;
    sync_parent_directory(path, DurableAuditFailureKind::RetentionRejected, None)
}

fn journal_sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut sidecar = path.as_os_str().to_os_string();
    sidecar.push(suffix);
    PathBuf::from(sidecar)
}

#[cfg(unix)]
fn sync_parent_directory(
    path: &Path,
    failure_kind: DurableAuditFailureKind,
    identity: Option<DurableAuditRecordIdentity>,
) -> DurableAuditSinkResult<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let directory = fs::File::open(parent).map_err(|error| {
        sink_failure(
            failure_kind,
            identity,
            format!("failed to open durable audit parent directory for sync: {error}"),
        )
    })?;
    directory.sync_all().map_err(|error| {
        sink_failure(
            failure_kind,
            identity,
            format!("failed to sync durable audit parent directory: {error}"),
        )
    })
}

#[cfg(not(unix))]
fn sync_parent_directory(
    _path: &Path,
    _failure_kind: DurableAuditFailureKind,
    _identity: Option<DurableAuditRecordIdentity>,
) -> DurableAuditSinkResult<()> {
    Ok(())
}
