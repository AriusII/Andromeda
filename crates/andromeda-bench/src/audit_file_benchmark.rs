use std::time::Instant;

use andromeda_core::{RequestId, SessionId};
use andromeda_observe::{
    CertificateIdentity, DurableAuditPrincipalBinding, DurableAuditReplayBehavior,
    DurableAuditReplayQuery, DurableAuditRetentionBoundary, DurableAuditSinkReport,
    DurableAuditWalSink, EventCorrelation, EventEnvelope, EventId, FileDurableAuditWalSink,
    PendingDurableAuditRecord, Permission, SecurityAuditOutcome, SecurityAuditTrace,
    SecurityPolicyVersionEvidence, SurfaceScope, TraceEvent, TraceId, UserPrincipal,
    UserPrincipalKind,
};

use crate::{
    BenchmarkError,
    harness::{BenchmarkTempDir as BenchTempDir, elapsed_micros},
};

pub const AUDIT_APPEND_FILE_SINK_WORKLOAD_ID: &str = "audit-append-file-sink-smoke";
pub const AUDIT_APPEND_FILE_SINK_HARNESS_SOURCE: &str = "observe-durable-audit-file-sink";
pub const AUDIT_APPEND_FILE_SINK_HARNESS_NAME: &str = "FileDurableAuditWalSink::append";

const FIRST_EVENT_ID: u128 = 20_000;
const FIRST_TRACE_ID: u128 = 30_000;
const FIRST_REQUEST_ID: u64 = 40_000;
const FIRST_SESSION_ID: u64 = 50_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditAppendFileSinkSmokeBenchmark {
    pub latencies_us: Vec<u64>,
    pub appended_records: usize,
    pub replayed_records: usize,
    pub durable_lsn: u64,
}

pub fn run_audit_append_file_sink_smoke_benchmark(
    samples: u32,
) -> Result<AuditAppendFileSinkSmokeBenchmark, BenchmarkError> {
    if samples == 0 {
        return Err(BenchmarkError::InsufficientSamplesForStatistics);
    }

    execute_audit_append_file_sink_smoke(samples).map_err(|_| BenchmarkError::HarnessFailed)
}

fn execute_audit_append_file_sink_smoke(
    samples: u32,
) -> Result<AuditAppendFileSinkSmokeBenchmark, AuditHarnessFailure> {
    let temp_dir = BenchTempDir::new("andromeda-bench-durable-audit").map_err(harness_failed)?;
    let journal_path = temp_dir.path().join("audit.wal");
    let mut sink = FileDurableAuditWalSink::open(&journal_path).map_err(harness_failed)?;
    let mut latencies_us = Vec::with_capacity(samples as usize);
    let mut durable_lsn = 0;

    for sample in 0..samples {
        let record = sample_record(sample)?;
        let started = Instant::now();
        let report = sink
            .append_durable_audit_record(record)
            .map_err(harness_failed)?;
        latencies_us.push(elapsed_micros(started));

        validate_report(report, sample)?;
        if report.evidence.record_lsn <= durable_lsn {
            return Err(AuditHarnessFailure);
        }
        durable_lsn = report.evidence.durable_lsn;
    }

    drop(sink);

    let reopened = FileDurableAuditWalSink::open(&journal_path).map_err(harness_failed)?;
    let replayed = reopened
        .replay(&DurableAuditReplayQuery::all())
        .map_err(harness_failed)?;
    if replayed.len() != samples as usize {
        return Err(AuditHarnessFailure);
    }
    if replayed
        .last()
        .is_some_and(|record| record.report.evidence.durable_lsn != durable_lsn)
    {
        return Err(AuditHarnessFailure);
    }

    Ok(AuditAppendFileSinkSmokeBenchmark {
        latencies_us,
        appended_records: samples as usize,
        replayed_records: replayed.len(),
        durable_lsn,
    })
}

fn sample_record(sample: u32) -> Result<PendingDurableAuditRecord, AuditHarnessFailure> {
    PendingDurableAuditRecord::new(
        u64::from(sample) + 1,
        principal_binding(sample),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        security_envelope(sample)?,
    )
    .map_err(harness_failed)
}

fn security_envelope(sample: u32) -> Result<EventEnvelope, AuditHarnessFailure> {
    let event_id = event_id(sample);
    let trace_id = trace_id(sample);
    let certificate = CertificateIdentity::new(
        format!("sha256:bench-durable-audit-{sample}"),
        "CN=andromeda-bench-durable-audit",
        SurfaceScope::Application,
    )
    .map_err(harness_failed)?;
    let principal = UserPrincipal::new(
        format!("user:bench-durable-audit-{sample}"),
        UserPrincipalKind::Human,
    )
    .map_err(harness_failed)?;
    let trace = SecurityAuditTrace::new(
        trace_id,
        SurfaceScope::Application,
        certificate,
        principal,
        Permission::ExecuteProcedure,
        SecurityAuditOutcome::Allowed,
        format!("benchmark durable audit append {sample}"),
    )
    .map_err(harness_failed)?;

    EventEnvelope::new(
        event_id,
        request_correlation(sample),
        TraceEvent::SecurityAudit(trace),
    )
    .map_err(harness_failed)
}

fn principal_binding(sample: u32) -> DurableAuditPrincipalBinding {
    DurableAuditPrincipalBinding {
        principal_id: format!("user:bench-durable-audit-{sample}"),
        certificate_fingerprint: Some(format!("sha256:bench-durable-audit-{sample}")),
        surface: Some(SurfaceScope::Application),
        permission: Some(Permission::ExecuteProcedure),
        policy_version: Some(SecurityPolicyVersionEvidence::bootstrap_v0()),
        request_id: Some(RequestId::new(FIRST_REQUEST_ID + u64::from(sample))),
        session_id: Some(SessionId::new(FIRST_SESSION_ID + u64::from(sample))),
    }
}

fn request_correlation(sample: u32) -> EventCorrelation {
    EventCorrelation {
        request_id: Some(RequestId::new(FIRST_REQUEST_ID + u64::from(sample))),
        session_id: Some(SessionId::new(FIRST_SESSION_ID + u64::from(sample))),
        contract_hash: None,
        catalog_version: None,
        catalog_object_id: None,
        transaction_id: None,
        durable_lsn: None,
        protocol: None,
    }
}

fn validate_report(report: DurableAuditSinkReport, sample: u32) -> Result<(), AuditHarnessFailure> {
    report.validate().map_err(harness_failed)?;
    if report.identity.event_id != event_id(sample)
        || report.identity.trace_id != trace_id(sample)
        || report.identity.sequence_number != u64::from(sample) + 1
        || report.evidence.record_lsn == 0
        || report.evidence.record_lsn != report.evidence.durable_lsn
        || report.evidence.checksum == 0
    {
        return Err(AuditHarnessFailure);
    }
    Ok(())
}

fn event_id(sample: u32) -> EventId {
    EventId::new(FIRST_EVENT_ID + u128::from(sample))
}

fn trace_id(sample: u32) -> TraceId {
    TraceId::new(FIRST_TRACE_ID + u128::from(sample))
}

#[derive(Debug)]
struct AuditHarnessFailure;

fn harness_failed<E>(_error: E) -> AuditHarnessFailure {
    AuditHarnessFailure
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_append_file_sink_smoke_appends_flushes_and_replays() {
        let result = run_audit_append_file_sink_smoke_benchmark(3).unwrap();

        assert_eq!(result.latencies_us.len(), 3);
        assert_eq!(result.appended_records, 3);
        assert_eq!(result.replayed_records, 3);
        assert!(result.durable_lsn >= 3);
        assert!(result.latencies_us.iter().all(|latency| *latency >= 1));
    }

    #[test]
    fn audit_append_file_sink_smoke_rejects_zero_samples() {
        assert_eq!(
            run_audit_append_file_sink_smoke_benchmark(0).unwrap_err(),
            BenchmarkError::InsufficientSamplesForStatistics
        );
    }
}
