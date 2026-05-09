use crate::{Permission, PermissionFamily, SecurityContractError, SecuritySurface};

pub const SECURITY_ADMISSION_V0_CONTRACT_ID: &str = "andromeda.security.admission.v0";
pub const SECURITY_ADMISSION_V0_SCHEMA_VERSION: u16 = 0;
pub const SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_ID: &str =
    "andromeda.audit.security_admission.v0";
pub const SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_VERSION: u16 = 0;

/// Canonical pre-dispatch security admission steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecurityAdmissionStepV0 {
    SurfaceBoundary,
    PrincipalBinding,
    ProcedureContract,
    PolicyEvidence,
    PermissionBoundary,
    DecisionEvidence,
}

pub const ALL_SECURITY_ADMISSION_V0_STEPS: [SecurityAdmissionStepV0; 6] = [
    SecurityAdmissionStepV0::SurfaceBoundary,
    SecurityAdmissionStepV0::PrincipalBinding,
    SecurityAdmissionStepV0::ProcedureContract,
    SecurityAdmissionStepV0::PolicyEvidence,
    SecurityAdmissionStepV0::PermissionBoundary,
    SecurityAdmissionStepV0::DecisionEvidence,
];

impl SecurityAdmissionStepV0 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SurfaceBoundary => "surface_boundary",
            Self::PrincipalBinding => "principal_binding",
            Self::ProcedureContract => "procedure_contract",
            Self::PolicyEvidence => "policy_evidence",
            Self::PermissionBoundary => "permission_boundary",
            Self::DecisionEvidence => "decision_evidence",
        }
    }

    pub const fn order(self) -> usize {
        match self {
            Self::SurfaceBoundary => 0,
            Self::PrincipalBinding => 1,
            Self::ProcedureContract => 2,
            Self::PolicyEvidence => 3,
            Self::PermissionBoundary => 4,
            Self::DecisionEvidence => 5,
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "surface_boundary" => Some(Self::SurfaceBoundary),
            "principal_binding" => Some(Self::PrincipalBinding),
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
    PrincipalBinding,
    ProcedureContract,
    PolicyVersion,
    PermissionFamily,
    OutcomeReason,
    Missing,
}

pub const ALL_SECURITY_ADMISSION_V0_EVIDENCE_CODES: [SecurityAdmissionEvidenceCodeV0; 7] = [
    SecurityAdmissionEvidenceCodeV0::SurfaceBoundary,
    SecurityAdmissionEvidenceCodeV0::PrincipalBinding,
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
            Self::PrincipalBinding => "principal_binding",
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
            "principal_binding" => Some(Self::PrincipalBinding),
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
    PrincipalBindingMissing,
    PermissionNotGranted,
    SurfacePermissionBoundaryMismatch,
    ProcedureContractBoundaryMismatch,
    PolicyVersionMissing,
    BoundaryPlaneMismatch,
}

pub const ALL_SECURITY_ADMISSION_V0_REASON_CODES: [SecurityAdmissionReasonCodeV0; 9] = [
    SecurityAdmissionReasonCodeV0::Satisfied,
    SecurityAdmissionReasonCodeV0::MissingEvidence,
    SecurityAdmissionReasonCodeV0::InvalidEvidenceShape,
    SecurityAdmissionReasonCodeV0::PrincipalBindingMissing,
    SecurityAdmissionReasonCodeV0::PermissionNotGranted,
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
            Self::PrincipalBindingMissing => "principal_binding_missing",
            Self::PermissionNotGranted => "permission_not_granted",
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
            "principal_binding_missing" => Some(Self::PrincipalBindingMissing),
            "permission_not_granted" => Some(Self::PermissionNotGranted),
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

pub const SURFACE_CLASS_ID_APPLICATION: &str = "application";
pub const SURFACE_CLASS_ID_ADMINISTRATION: &str = "administration";
pub const SURFACE_CLASS_ID_HADR: &str = "hadr";
pub const SURFACE_CLASS_ID_RECOVERY: &str = "recovery";
pub const SURFACE_CLASS_ID_FORENSIC: &str = "forensic";
pub const SURFACE_CLASS_ID_MONITORING: &str = "monitoring";

/// Runtime-free class of work requested before dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurfaceClass {
    Application,
    Administration,
    Hadr,
    Recovery,
    Forensic,
    Monitoring,
}

pub const ALL_SURFACE_CLASSES: [SurfaceClass; 6] = [
    SurfaceClass::Application,
    SurfaceClass::Administration,
    SurfaceClass::Hadr,
    SurfaceClass::Recovery,
    SurfaceClass::Forensic,
    SurfaceClass::Monitoring,
];

impl SurfaceClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Application => SURFACE_CLASS_ID_APPLICATION,
            Self::Administration => SURFACE_CLASS_ID_ADMINISTRATION,
            Self::Hadr => SURFACE_CLASS_ID_HADR,
            Self::Recovery => SURFACE_CLASS_ID_RECOVERY,
            Self::Forensic => SURFACE_CLASS_ID_FORENSIC,
            Self::Monitoring => SURFACE_CLASS_ID_MONITORING,
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            SURFACE_CLASS_ID_APPLICATION => Some(Self::Application),
            SURFACE_CLASS_ID_ADMINISTRATION => Some(Self::Administration),
            SURFACE_CLASS_ID_HADR => Some(Self::Hadr),
            SURFACE_CLASS_ID_RECOVERY => Some(Self::Recovery),
            SURFACE_CLASS_ID_FORENSIC => Some(Self::Forensic),
            SURFACE_CLASS_ID_MONITORING => Some(Self::Monitoring),
            _ => None,
        }
    }

    pub const fn is_application_work(self) -> bool {
        matches!(self, Self::Application)
    }

    pub const fn is_privileged_work(self) -> bool {
        !self.is_application_work()
    }

    pub const fn permits_surface(self, surface: SecuritySurface) -> bool {
        match self {
            Self::Application => matches!(surface, SecuritySurface::Application),
            Self::Administration => matches!(surface, SecuritySurface::Administration),
            Self::Hadr => matches!(surface, SecuritySurface::Cluster),
            Self::Recovery | Self::Forensic => {
                matches!(
                    surface,
                    SecuritySurface::Administration | SecuritySurface::BackupAgent
                )
            },
            Self::Monitoring => matches!(surface, SecuritySurface::MonitoringAgent),
        }
    }

    pub const fn permits_permission(self, permission: Permission) -> bool {
        match self {
            Self::Application => matches!(permission.family(), PermissionFamily::Application),
            Self::Administration => matches!(
                permission.family(),
                PermissionFamily::Definition
                    | PermissionFamily::Diagnostics
                    | PermissionFamily::Security
            ),
            Self::Hadr => matches!(permission.family(), PermissionFamily::Cluster),
            Self::Recovery => matches!(permission, Permission::Backup | Permission::Restore),
            Self::Forensic => matches!(permission, Permission::ForensicStart),
            Self::Monitoring => matches!(permission.family(), PermissionFamily::Diagnostics),
        }
    }
}

impl core::fmt::Display for SurfaceClass {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Validated permission request after surface class and permission boundaries match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PermissionRequest {
    surface: SecuritySurface,
    class: SurfaceClass,
    permission: Permission,
}

impl PermissionRequest {
    pub const fn new(
        surface: SecuritySurface,
        class: SurfaceClass,
        permission: Permission,
    ) -> Result<Self, SecurityContractError> {
        match validate_permission_request(surface, class, permission) {
            Ok(()) => Ok(Self {
                surface,
                class,
                permission,
            }),
            Err(error) => Err(error),
        }
    }

    pub const fn surface(self) -> SecuritySurface {
        self.surface
    }

    pub const fn class(self) -> SurfaceClass {
        self.class
    }

    pub const fn permission(self) -> Permission {
        self.permission
    }

    pub const fn is_application_work(self) -> bool {
        matches!(self.surface, SecuritySurface::Application)
            && self.class.is_application_work()
            && matches!(self.permission.family(), PermissionFamily::Application)
    }
}

/// Runtime-free surface and permission admission decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AdmissionDecision {
    surface: SecuritySurface,
    class: SurfaceClass,
    permission: Permission,
    outcome: SecurityAdmissionOutcomeV0,
    reason_code: SecurityAdmissionReasonCodeV0,
}

impl AdmissionDecision {
    pub const fn evaluate(
        surface: SecuritySurface,
        class: SurfaceClass,
        permission: Permission,
    ) -> Self {
        match validate_permission_request(surface, class, permission) {
            Ok(()) => Self {
                surface,
                class,
                permission,
                outcome: SecurityAdmissionOutcomeV0::Allowed,
                reason_code: SecurityAdmissionReasonCodeV0::Satisfied,
            },
            Err(SecurityContractError::SurfaceClassBoundaryMismatch) => Self::denied(
                surface,
                class,
                permission,
                SecurityAdmissionReasonCodeV0::BoundaryPlaneMismatch,
            ),
            Err(SecurityContractError::SurfacePermissionBoundaryMismatch) => Self::denied(
                surface,
                class,
                permission,
                SecurityAdmissionReasonCodeV0::SurfacePermissionBoundaryMismatch,
            ),
            Err(_) => Self::denied(
                surface,
                class,
                permission,
                SecurityAdmissionReasonCodeV0::InvalidEvidenceShape,
            ),
        }
    }

    pub const fn admitted(request: PermissionRequest) -> Self {
        Self {
            surface: request.surface(),
            class: request.class(),
            permission: request.permission(),
            outcome: SecurityAdmissionOutcomeV0::Allowed,
            reason_code: SecurityAdmissionReasonCodeV0::Satisfied,
        }
    }

    const fn denied(
        surface: SecuritySurface,
        class: SurfaceClass,
        permission: Permission,
        reason_code: SecurityAdmissionReasonCodeV0,
    ) -> Self {
        Self {
            surface,
            class,
            permission,
            outcome: SecurityAdmissionOutcomeV0::Denied,
            reason_code,
        }
    }

    pub const fn surface(self) -> SecuritySurface {
        self.surface
    }

    pub const fn class(self) -> SurfaceClass {
        self.class
    }

    pub const fn permission(self) -> Permission {
        self.permission
    }

    pub const fn outcome(self) -> SecurityAdmissionOutcomeV0 {
        self.outcome
    }

    pub const fn reason_code(self) -> SecurityAdmissionReasonCodeV0 {
        self.reason_code
    }

    pub const fn is_admitted(self) -> bool {
        matches!(self.outcome, SecurityAdmissionOutcomeV0::Allowed)
    }

    pub const fn is_denied(self) -> bool {
        !self.is_admitted()
    }

    pub const fn request(self) -> Option<PermissionRequest> {
        if self.is_admitted() {
            Some(PermissionRequest {
                surface: self.surface,
                class: self.class,
                permission: self.permission,
            })
        } else {
            None
        }
    }

    pub const fn security_admission_v0(self) -> SecurityAdmissionV0 {
        if self.is_admitted() {
            SecurityAdmissionV0::allowed(
                SecurityAdmissionStepV0::PermissionBoundary,
                SecurityAdmissionEvidenceCodeV0::PermissionFamily,
            )
        } else {
            match self.reason_code {
                SecurityAdmissionReasonCodeV0::BoundaryPlaneMismatch => {
                    SecurityAdmissionV0::denied(
                        SecurityAdmissionStepV0::SurfaceBoundary,
                        SecurityAdmissionEvidenceCodeV0::SurfaceBoundary,
                        self.reason_code,
                    )
                },
                _ => SecurityAdmissionV0::denied(
                    SecurityAdmissionStepV0::PermissionBoundary,
                    SecurityAdmissionEvidenceCodeV0::PermissionFamily,
                    self.reason_code,
                ),
            }
        }
    }
}

const fn validate_permission_request(
    surface: SecuritySurface,
    class: SurfaceClass,
    permission: Permission,
) -> Result<(), SecurityContractError> {
    if !class.permits_surface(surface) {
        return Err(SecurityContractError::SurfaceClassBoundaryMismatch);
    }

    if !surface.permits_permission(permission) || !class.permits_permission(permission) {
        return Err(SecurityContractError::SurfacePermissionBoundaryMismatch);
    }

    Ok(())
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

/// Runtime-free audit projection for a security admission decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SecurityAdmissionAuditEventV0 {
    admission: SecurityAdmissionV0,
    surface: SecuritySurface,
    class: SurfaceClass,
    permission: Permission,
    policy_evidence_present: bool,
}

impl SecurityAdmissionAuditEventV0 {
    pub const fn new(
        admission: SecurityAdmissionV0,
        surface: SecuritySurface,
        class: SurfaceClass,
        permission: Permission,
        policy_evidence_present: bool,
    ) -> Self {
        Self {
            admission,
            surface,
            class,
            permission,
            policy_evidence_present,
        }
    }

    pub const fn schema_id(self) -> &'static str {
        SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_ID
    }

    pub const fn schema_version(self) -> u16 {
        SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_VERSION
    }

    pub const fn family(self) -> &'static str {
        "security_admission"
    }

    pub const fn phase(self) -> &'static str {
        "pre_transaction_admission"
    }

    pub const fn admission(self) -> SecurityAdmissionV0 {
        self.admission
    }

    pub const fn step(self) -> SecurityAdmissionStepV0 {
        self.admission.step()
    }

    pub const fn evidence(self) -> SecurityAdmissionEvidenceCodeV0 {
        self.admission.evidence()
    }

    pub const fn outcome(self) -> SecurityAdmissionOutcomeV0 {
        self.admission.outcome()
    }

    pub const fn reason_code(self) -> SecurityAdmissionReasonCodeV0 {
        self.admission.reason_code()
    }

    pub const fn surface(self) -> SecuritySurface {
        self.surface
    }

    pub const fn class(self) -> SurfaceClass {
        self.class
    }

    pub const fn permission(self) -> Permission {
        self.permission
    }

    pub const fn policy_evidence_present(self) -> bool {
        self.policy_evidence_present
    }

    pub const fn is_allowed(self) -> bool {
        self.admission.is_allowed()
    }

    pub const fn is_denied(self) -> bool {
        !self.is_allowed()
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
                SecurityAdmissionStepV0::PrincipalBinding,
                "principal_binding",
                1,
            ),
            (
                SecurityAdmissionStepV0::ProcedureContract,
                "procedure_contract",
                2,
            ),
            (
                SecurityAdmissionStepV0::PolicyEvidence,
                "policy_evidence",
                3,
            ),
            (
                SecurityAdmissionStepV0::PermissionBoundary,
                "permission_boundary",
                4,
            ),
            (
                SecurityAdmissionStepV0::DecisionEvidence,
                "decision_evidence",
                5,
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
                SecurityAdmissionEvidenceCodeV0::PrincipalBinding,
                "principal_binding",
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
                SecurityAdmissionReasonCodeV0::PrincipalBindingMissing,
                "principal_binding_missing",
            ),
            (
                SecurityAdmissionReasonCodeV0::PermissionNotGranted,
                "permission_not_granted",
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
