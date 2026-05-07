use super::{DurableAuditEventFamily, DurableAuditPrincipalBinding};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurableAuditPolicyEvidenceRequirement {
    NotRequired,
    PermissionedCriticalDecision,
}

impl DurableAuditPolicyEvidenceRequirement {
    pub const fn requires_policy_evidence(self) -> bool {
        matches!(self, Self::PermissionedCriticalDecision)
    }
}

pub fn classify_policy_evidence_requirement(
    family: DurableAuditEventFamily,
    binding: &DurableAuditPrincipalBinding,
) -> DurableAuditPolicyEvidenceRequirement {
    match family {
        DurableAuditEventFamily::SecurityDecision
        | DurableAuditEventFamily::AdminDecision
        | DurableAuditEventFamily::HadrDecision
        | DurableAuditEventFamily::BackupDecision
        | DurableAuditEventFamily::RestoreDecision
        | DurableAuditEventFamily::ForensicDecision => {
            DurableAuditPolicyEvidenceRequirement::PermissionedCriticalDecision
        }
        DurableAuditEventFamily::CatalogDecision if binding.permission.is_some() => {
            DurableAuditPolicyEvidenceRequirement::PermissionedCriticalDecision
        }
        DurableAuditEventFamily::RecoveryDecision if binding.permission.is_some() => {
            DurableAuditPolicyEvidenceRequirement::PermissionedCriticalDecision
        }
        DurableAuditEventFamily::AdmissionDecision
        | DurableAuditEventFamily::CatalogDecision
        | DurableAuditEventFamily::RecoveryDecision
        | DurableAuditEventFamily::GenericAudit => {
            DurableAuditPolicyEvidenceRequirement::NotRequired
        }
    }
}
