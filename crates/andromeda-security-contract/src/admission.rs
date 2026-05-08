use crate::{PermissionFamily, SecuritySurface};

pub const SECURITY_ADMISSION_V0_CONTRACT_ID: &str = "andromeda.security.admission.v0";
pub const SECURITY_ADMISSION_V0_SCHEMA_VERSION: u16 = 0;

/// Canonical pre-dispatch security admission steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecurityAdmissionStepV0 {
    SurfaceBoundary,
    ProcedureContract,
    PolicyEvidence,
    PermissionBoundary,
    DecisionEvidence,
}

pub const ALL_SECURITY_ADMISSION_V0_STEPS: [SecurityAdmissionStepV0; 5] = [
    SecurityAdmissionStepV0::SurfaceBoundary,
    SecurityAdmissionStepV0::ProcedureContract,
    SecurityAdmissionStepV0::PolicyEvidence,
    SecurityAdmissionStepV0::PermissionBoundary,
    SecurityAdmissionStepV0::DecisionEvidence,
];

impl SecurityAdmissionStepV0 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SurfaceBoundary => "surface_boundary",
            Self::ProcedureContract => "procedure_contract",
            Self::PolicyEvidence => "policy_evidence",
            Self::PermissionBoundary => "permission_boundary",
            Self::DecisionEvidence => "decision_evidence",
        }
    }

    pub const fn order(self) -> usize {
        match self {
            Self::SurfaceBoundary => 0,
            Self::ProcedureContract => 1,
            Self::PolicyEvidence => 2,
            Self::PermissionBoundary => 3,
            Self::DecisionEvidence => 4,
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "surface_boundary" => Some(Self::SurfaceBoundary),
            "procedure_contract" => Some(Self::ProcedureContract),
            "policy_evidence" => Some(Self::PolicyEvidence),
            "permission_boundary" => Some(Self::PermissionBoundary),
            "decision_evidence" => Some(Self::DecisionEvidence),
            _ => None,
        }
    }
}

impl core::fmt::Display for SecurityAdmissionStepV0 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Evidence shape codes accepted by the V0 admission artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecurityAdmissionEvidenceCodeV0 {
    SurfaceBoundary,
    ProcedureContract,
    PolicyVersion,
    PermissionFamily,
    OutcomeReason,
    Missing,
}

pub const ALL_SECURITY_ADMISSION_V0_EVIDENCE_CODES: [SecurityAdmissionEvidenceCodeV0; 6] = [
    SecurityAdmissionEvidenceCodeV0::SurfaceBoundary,
    SecurityAdmissionEvidenceCodeV0::ProcedureContract,
    SecurityAdmissionEvidenceCodeV0::PolicyVersion,
    SecurityAdmissionEvidenceCodeV0::PermissionFamily,
    SecurityAdmissionEvidenceCodeV0::OutcomeReason,
    SecurityAdmissionEvidenceCodeV0::Missing,
];

impl SecurityAdmissionEvidenceCodeV0 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SurfaceBoundary => "surface_boundary",
            Self::ProcedureContract => "procedure_contract",
            Self::PolicyVersion => "policy_version",
            Self::PermissionFamily => "permission_family",
            Self::OutcomeReason => "outcome_reason",
            Self::Missing => "missing",
        }
    }

    pub const fn is_missing(self) -> bool {
        matches!(self, Self::Missing)
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "surface_boundary" => Some(Self::SurfaceBoundary),
            "procedure_contract" => Some(Self::ProcedureContract),
            "policy_version" => Some(Self::PolicyVersion),
            "permission_family" => Some(Self::PermissionFamily),
            "outcome_reason" => Some(Self::OutcomeReason),
            "missing" => Some(Self::Missing),
            _ => None,
        }
    }
}

impl core::fmt::Display for SecurityAdmissionEvidenceCodeV0 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Admission outcome codes. Absence of required evidence must resolve to `Denied`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecurityAdmissionOutcomeV0 {
    Allowed,
    Denied,
}

pub const ALL_SECURITY_ADMISSION_V0_OUTCOMES: [SecurityAdmissionOutcomeV0; 2] = [
    SecurityAdmissionOutcomeV0::Allowed,
    SecurityAdmissionOutcomeV0::Denied,
];

impl SecurityAdmissionOutcomeV0 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Denied => "denied",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "allowed" => Some(Self::Allowed),
            "denied" => Some(Self::Denied),
            _ => None,
        }
    }
}

impl core::fmt::Display for SecurityAdmissionOutcomeV0 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Stable reason codes for a V0 security admission artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecurityAdmissionReasonCodeV0 {
    Satisfied,
    MissingEvidence,
    InvalidEvidenceShape,
    SurfacePermissionBoundaryMismatch,
    ProcedureContractBoundaryMismatch,
    PolicyVersionMissing,
    BoundaryPlaneMismatch,
}

pub const ALL_SECURITY_ADMISSION_V0_REASON_CODES: [SecurityAdmissionReasonCodeV0; 7] = [
    SecurityAdmissionReasonCodeV0::Satisfied,
    SecurityAdmissionReasonCodeV0::MissingEvidence,
    SecurityAdmissionReasonCodeV0::InvalidEvidenceShape,
    SecurityAdmissionReasonCodeV0::SurfacePermissionBoundaryMismatch,
    SecurityAdmissionReasonCodeV0::ProcedureContractBoundaryMismatch,
    SecurityAdmissionReasonCodeV0::PolicyVersionMissing,
    SecurityAdmissionReasonCodeV0::BoundaryPlaneMismatch,
];

impl SecurityAdmissionReasonCodeV0 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Satisfied => "satisfied",
            Self::MissingEvidence => "missing_evidence",
            Self::InvalidEvidenceShape => "invalid_evidence_shape",
            Self::SurfacePermissionBoundaryMismatch => "surface_permission_boundary_mismatch",
            Self::ProcedureContractBoundaryMismatch => "procedure_contract_boundary_mismatch",
            Self::PolicyVersionMissing => "policy_version_missing",
            Self::BoundaryPlaneMismatch => "boundary_plane_mismatch",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "satisfied" => Some(Self::Satisfied),
            "missing_evidence" => Some(Self::MissingEvidence),
            "invalid_evidence_shape" => Some(Self::InvalidEvidenceShape),
            "surface_permission_boundary_mismatch" => Some(Self::SurfacePermissionBoundaryMismatch),
            "procedure_contract_boundary_mismatch" => Some(Self::ProcedureContractBoundaryMismatch),
            "policy_version_missing" => Some(Self::PolicyVersionMissing),
            "boundary_plane_mismatch" => Some(Self::BoundaryPlaneMismatch),
            _ => None,
        }
    }
}

impl core::fmt::Display for SecurityAdmissionReasonCodeV0 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Typed admission boundary codes for externally visible security planes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecurityAdmissionBoundaryV0 {
    Application,
    Administration,
    Cluster,
    Backup,
    Monitoring,
}

pub const ALL_SECURITY_ADMISSION_V0_BOUNDARIES: [SecurityAdmissionBoundaryV0; 5] = [
    SecurityAdmissionBoundaryV0::Application,
    SecurityAdmissionBoundaryV0::Administration,
    SecurityAdmissionBoundaryV0::Cluster,
    SecurityAdmissionBoundaryV0::Backup,
    SecurityAdmissionBoundaryV0::Monitoring,
];

impl SecurityAdmissionBoundaryV0 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Application => "application",
            Self::Administration => "administration",
            Self::Cluster => "cluster",
            Self::Backup => "backup",
            Self::Monitoring => "monitoring",
        }
    }

    pub const fn surface(self) -> SecuritySurface {
        match self {
            Self::Application => SecuritySurface::Application,
            Self::Administration => SecuritySurface::Administration,
            Self::Cluster => SecuritySurface::Cluster,
            Self::Backup => SecuritySurface::BackupAgent,
            Self::Monitoring => SecuritySurface::MonitoringAgent,
        }
    }

    pub const fn permits_family(self, family: PermissionFamily) -> bool {
        self.surface().permits_family(family)
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "application" => Some(Self::Application),
            "administration" => Some(Self::Administration),
            "cluster" => Some(Self::Cluster),
            "backup" => Some(Self::Backup),
            "monitoring" => Some(Self::Monitoring),
            _ => None,
        }
    }
}

impl core::fmt::Display for SecurityAdmissionBoundaryV0 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Runtime-free V0 admission artifact containing only stable codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SecurityAdmissionV0 {
    step: SecurityAdmissionStepV0,
    evidence: SecurityAdmissionEvidenceCodeV0,
    outcome: SecurityAdmissionOutcomeV0,
    reason_code: SecurityAdmissionReasonCodeV0,
}

impl SecurityAdmissionV0 {
    pub const fn new(
        step: SecurityAdmissionStepV0,
        evidence: SecurityAdmissionEvidenceCodeV0,
        outcome: SecurityAdmissionOutcomeV0,
        reason_code: SecurityAdmissionReasonCodeV0,
    ) -> Self {
        match evidence {
            SecurityAdmissionEvidenceCodeV0::Missing => Self {
                step,
                evidence,
                outcome: SecurityAdmissionOutcomeV0::Denied,
                reason_code: SecurityAdmissionReasonCodeV0::MissingEvidence,
            },
            _ => Self {
                step,
                evidence,
                outcome,
                reason_code,
            },
        }
    }

    pub const fn allowed(
        step: SecurityAdmissionStepV0,
        evidence: SecurityAdmissionEvidenceCodeV0,
    ) -> Self {
        Self::new(
            step,
            evidence,
            SecurityAdmissionOutcomeV0::Allowed,
            SecurityAdmissionReasonCodeV0::Satisfied,
        )
    }

    pub const fn denied(
        step: SecurityAdmissionStepV0,
        evidence: SecurityAdmissionEvidenceCodeV0,
        reason_code: SecurityAdmissionReasonCodeV0,
    ) -> Self {
        Self::new(
            step,
            evidence,
            SecurityAdmissionOutcomeV0::Denied,
            reason_code,
        )
    }

    pub const fn missing_evidence(step: SecurityAdmissionStepV0) -> Self {
        Self::new(
            step,
            SecurityAdmissionEvidenceCodeV0::Missing,
            SecurityAdmissionOutcomeV0::Denied,
            SecurityAdmissionReasonCodeV0::MissingEvidence,
        )
    }

    pub const fn step(self) -> SecurityAdmissionStepV0 {
        self.step
    }

    pub const fn evidence(self) -> SecurityAdmissionEvidenceCodeV0 {
        self.evidence
    }

    pub const fn outcome(self) -> SecurityAdmissionOutcomeV0 {
        self.outcome
    }

    pub const fn reason_code(self) -> SecurityAdmissionReasonCodeV0 {
        self.reason_code
    }

    pub const fn is_allowed(self) -> bool {
        matches!(self.outcome, SecurityAdmissionOutcomeV0::Allowed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_admission_steps_have_canonical_order_and_codes() {
        let expected = [
            (
                SecurityAdmissionStepV0::SurfaceBoundary,
                "surface_boundary",
                0,
            ),
            (
                SecurityAdmissionStepV0::ProcedureContract,
                "procedure_contract",
                1,
            ),
            (
                SecurityAdmissionStepV0::PolicyEvidence,
                "policy_evidence",
                2,
            ),
            (
                SecurityAdmissionStepV0::PermissionBoundary,
                "permission_boundary",
                3,
            ),
            (
                SecurityAdmissionStepV0::DecisionEvidence,
                "decision_evidence",
                4,
            ),
        ];

        for (index, (step, code, order)) in expected.into_iter().enumerate() {
            assert_eq!(ALL_SECURITY_ADMISSION_V0_STEPS[index], step);
            assert_eq!(step.as_str(), code);
            assert_eq!(step.order(), order);
            assert_eq!(SecurityAdmissionStepV0::from_code(code), Some(step));
        }
    }

    #[test]
    fn test_admission_code_sets_have_canonical_order() {
        let evidence_codes = [
            (
                SecurityAdmissionEvidenceCodeV0::SurfaceBoundary,
                "surface_boundary",
            ),
            (
                SecurityAdmissionEvidenceCodeV0::ProcedureContract,
                "procedure_contract",
            ),
            (
                SecurityAdmissionEvidenceCodeV0::PolicyVersion,
                "policy_version",
            ),
            (
                SecurityAdmissionEvidenceCodeV0::PermissionFamily,
                "permission_family",
            ),
            (
                SecurityAdmissionEvidenceCodeV0::OutcomeReason,
                "outcome_reason",
            ),
            (SecurityAdmissionEvidenceCodeV0::Missing, "missing"),
        ];
        for (index, (evidence, code)) in evidence_codes.into_iter().enumerate() {
            assert_eq!(ALL_SECURITY_ADMISSION_V0_EVIDENCE_CODES[index], evidence);
            assert_eq!(evidence.as_str(), code);
            assert_eq!(
                SecurityAdmissionEvidenceCodeV0::from_code(code),
                Some(evidence)
            );
        }

        let outcomes = [
            (SecurityAdmissionOutcomeV0::Allowed, "allowed"),
            (SecurityAdmissionOutcomeV0::Denied, "denied"),
        ];
        for (index, (outcome, code)) in outcomes.into_iter().enumerate() {
            assert_eq!(ALL_SECURITY_ADMISSION_V0_OUTCOMES[index], outcome);
            assert_eq!(outcome.as_str(), code);
            assert_eq!(SecurityAdmissionOutcomeV0::from_code(code), Some(outcome));
        }

        let reason_codes = [
            (SecurityAdmissionReasonCodeV0::Satisfied, "satisfied"),
            (
                SecurityAdmissionReasonCodeV0::MissingEvidence,
                "missing_evidence",
            ),
            (
                SecurityAdmissionReasonCodeV0::InvalidEvidenceShape,
                "invalid_evidence_shape",
            ),
            (
                SecurityAdmissionReasonCodeV0::SurfacePermissionBoundaryMismatch,
                "surface_permission_boundary_mismatch",
            ),
            (
                SecurityAdmissionReasonCodeV0::ProcedureContractBoundaryMismatch,
                "procedure_contract_boundary_mismatch",
            ),
            (
                SecurityAdmissionReasonCodeV0::PolicyVersionMissing,
                "policy_version_missing",
            ),
            (
                SecurityAdmissionReasonCodeV0::BoundaryPlaneMismatch,
                "boundary_plane_mismatch",
            ),
        ];
        for (index, (reason_code, code)) in reason_codes.into_iter().enumerate() {
            assert_eq!(ALL_SECURITY_ADMISSION_V0_REASON_CODES[index], reason_code);
            assert_eq!(reason_code.as_str(), code);
            assert_eq!(
                SecurityAdmissionReasonCodeV0::from_code(code),
                Some(reason_code)
            );
        }
    }
    #[test]
    fn test_missing_evidence_shape_fails_closed() {
        let requested_allow = SecurityAdmissionV0::new(
            SecurityAdmissionStepV0::PolicyEvidence,
            SecurityAdmissionEvidenceCodeV0::Missing,
            SecurityAdmissionOutcomeV0::Allowed,
            SecurityAdmissionReasonCodeV0::Satisfied,
        );

        assert_eq!(
            requested_allow.outcome(),
            SecurityAdmissionOutcomeV0::Denied
        );
        assert_eq!(
            requested_allow.reason_code(),
            SecurityAdmissionReasonCodeV0::MissingEvidence
        );
        assert_eq!(
            requested_allow.evidence(),
            SecurityAdmissionEvidenceCodeV0::Missing
        );
        assert!(!requested_allow.is_allowed());

        let explicit_missing =
            SecurityAdmissionV0::missing_evidence(SecurityAdmissionStepV0::ProcedureContract);
        assert_eq!(
            explicit_missing.outcome(),
            SecurityAdmissionOutcomeV0::Denied
        );
        assert_eq!(
            explicit_missing.reason_code(),
            SecurityAdmissionReasonCodeV0::MissingEvidence
        );
    }

    #[test]
    fn test_admission_boundaries_stay_typed() {
        let expected = [
            (
                SecurityAdmissionBoundaryV0::Application,
                SecuritySurface::Application,
            ),
            (
                SecurityAdmissionBoundaryV0::Administration,
                SecuritySurface::Administration,
            ),
            (
                SecurityAdmissionBoundaryV0::Cluster,
                SecuritySurface::Cluster,
            ),
            (
                SecurityAdmissionBoundaryV0::Backup,
                SecuritySurface::BackupAgent,
            ),
            (
                SecurityAdmissionBoundaryV0::Monitoring,
                SecuritySurface::MonitoringAgent,
            ),
        ];

        for (index, (boundary, surface)) in expected.into_iter().enumerate() {
            assert_eq!(ALL_SECURITY_ADMISSION_V0_BOUNDARIES[index], boundary);
            assert_eq!(boundary.surface(), surface);
            assert_eq!(
                SecurityAdmissionBoundaryV0::from_code(boundary.as_str()),
                Some(boundary)
            );
        }

        assert!(
            SecurityAdmissionBoundaryV0::Application.permits_family(PermissionFamily::Application)
        );
        assert!(
            !SecurityAdmissionBoundaryV0::Application.permits_family(PermissionFamily::Security)
        );
        assert!(
            SecurityAdmissionBoundaryV0::Administration.permits_family(PermissionFamily::Security)
        );
        assert!(
            !SecurityAdmissionBoundaryV0::Administration.permits_family(PermissionFamily::Cluster)
        );
        assert!(SecurityAdmissionBoundaryV0::Cluster.permits_family(PermissionFamily::Cluster));
        assert!(!SecurityAdmissionBoundaryV0::Cluster.permits_family(PermissionFamily::Security));
        assert!(SecurityAdmissionBoundaryV0::Backup.permits_family(PermissionFamily::Recovery));
        assert!(!SecurityAdmissionBoundaryV0::Backup.permits_family(PermissionFamily::Cluster));
        assert!(
            SecurityAdmissionBoundaryV0::Monitoring.permits_family(PermissionFamily::Diagnostics)
        );
        assert!(
            !SecurityAdmissionBoundaryV0::Monitoring.permits_family(PermissionFamily::Recovery)
        );
    }
}
