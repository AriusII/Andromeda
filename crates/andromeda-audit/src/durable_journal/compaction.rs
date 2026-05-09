use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use super::{
    DurableAuditCompactionReport, DurableAuditFailureKind, DurableAuditReplayQuery,
    DurableAuditReplayRecord, DurableAuditRetentionManager, DurableAuditRetentionPolicy,
    DurableAuditSinkResult, DurableAuditWalSegmentArchiveProof,
    journal_format::{
        journal_line, remove_journal_chain_anchor, replay_durable_audit_journal,
        write_journal_chain_anchor,
    },
    journal_mutation::{acquire_mutation_lock, journal_sidecar_path, publish_compacted_journal},
    sink_failure,
};

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
