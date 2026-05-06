use super::{
    DurableAuditEventFamily, DurableAuditFailureKind, DurableAuditPrincipalBinding,
    DurableAuditRetentionBoundary, DurableAuditSinkReport, DurableAuditSinkResult,
    DurableAuditWalSink, PendingDurableAuditRecord, sink_failure,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableAuditDecisionGate {
    pub expected_family: DurableAuditEventFamily,
}

impl DurableAuditDecisionGate {
    pub const fn new(expected_family: DurableAuditEventFamily) -> Self {
        Self { expected_family }
    }

    pub fn append_and_prove<S>(
        self,
        sink: &mut S,
        record: PendingDurableAuditRecord,
    ) -> DurableAuditSinkResult<DurableAuditVisibleDecisionProof>
    where
        S: DurableAuditWalSink + ?Sized,
    {
        if !self.expected_family.requires_wal_before_visible_decision() {
            return Err(gate_failure(
                None,
                "durable audit visible decision gate requires a visible decision family",
            ));
        }

        let identity = record.identity;
        record.validate().map_err(|error| {
            gate_failure(
                Some(identity),
                format!(
                    "durable audit visible decision gate rejected invalid record: {}",
                    error.message()
                ),
            )
        })?;
        if identity.family != self.expected_family {
            return Err(gate_failure(
                Some(identity),
                "durable audit visible decision gate record family does not match expected family",
            ));
        }

        let principal_binding = record.principal_binding.clone();
        let report = sink.append_durable_audit_record(record)?;
        self.prove_visible_decision(report, principal_binding)
    }

    pub fn prove_visible_decision(
        self,
        report: DurableAuditSinkReport,
        principal_binding: DurableAuditPrincipalBinding,
    ) -> DurableAuditSinkResult<DurableAuditVisibleDecisionProof> {
        if !self.expected_family.requires_wal_before_visible_decision() {
            return Err(gate_failure(
                Some(report.identity),
                "durable audit visible decision gate requires a visible decision family",
            ));
        }
        if report.identity.family != self.expected_family {
            return Err(gate_failure(
                Some(report.identity),
                "durable audit visible decision proof family does not match gate expectation",
            ));
        }
        report.validate().map_err(|error| {
            gate_failure(
                Some(report.identity),
                format!(
                    "durable audit visible decision proof requires flushed WAL evidence: {}",
                    error.message()
                ),
            )
        })?;
        principal_binding.validate().map_err(|error| {
            gate_failure(
                Some(report.identity),
                format!(
                    "durable audit visible decision proof rejected principal binding: {}",
                    error.message()
                ),
            )
        })?;
        if principal_binding.surface.is_none() || principal_binding.permission.is_none() {
            return Err(gate_failure(
                Some(report.identity),
                "durable audit visible decision proof requires explicit surface and permission evidence",
            ));
        }
        if !retention_boundary_is_compatible(self.expected_family, report.retention) {
            return Err(gate_failure(
                Some(report.identity),
                "durable audit visible decision proof retention boundary is not compatible with the decision family",
            ));
        }
        if !report.replay_behavior.is_visible_decision_evidence() {
            return Err(gate_failure(
                Some(report.identity),
                "durable audit visible decision proof requires explicit visible-decision replay behavior",
            ));
        }

        Ok(DurableAuditVisibleDecisionProof {
            report,
            principal_binding,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditVisibleDecisionProof {
    pub report: DurableAuditSinkReport,
    pub principal_binding: DurableAuditPrincipalBinding,
}

fn retention_boundary_is_compatible(
    family: DurableAuditEventFamily,
    boundary: DurableAuditRetentionBoundary,
) -> bool {
    match family {
        DurableAuditEventFamily::SecurityDecision
        | DurableAuditEventFamily::AdminDecision
        | DurableAuditEventFamily::HadrDecision => matches!(
            boundary,
            DurableAuditRetentionBoundary::SecurityPolicy
                | DurableAuditRetentionBoundary::ForensicHold
        ),
        DurableAuditEventFamily::CatalogDecision => matches!(
            boundary,
            DurableAuditRetentionBoundary::CatalogVersion
                | DurableAuditRetentionBoundary::ForensicHold
        ),
        DurableAuditEventFamily::BackupDecision | DurableAuditEventFamily::RestoreDecision => {
            matches!(
                boundary,
                DurableAuditRetentionBoundary::WalSegment
                    | DurableAuditRetentionBoundary::ForensicHold
            )
        }
        DurableAuditEventFamily::ForensicDecision => {
            boundary == DurableAuditRetentionBoundary::ForensicHold
        }
        DurableAuditEventFamily::AdmissionDecision
        | DurableAuditEventFamily::RecoveryDecision
        | DurableAuditEventFamily::GenericAudit => false,
    }
}

fn gate_failure(
    identity: Option<super::DurableAuditRecordIdentity>,
    reason: impl Into<String>,
) -> super::DurableAuditSinkFailure {
    sink_failure(
        DurableAuditFailureKind::ValidationRejected,
        identity,
        reason,
    )
}
