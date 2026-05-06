use super::DurableAuditEventFamily;
use crate::events::{AdminOperation, TraceEvent};

pub fn durable_audit_family(event: &TraceEvent) -> Option<DurableAuditEventFamily> {
    match event {
        TraceEvent::SecurityAudit(_) | TraceEvent::AuthorizationDenied(_) => {
            Some(DurableAuditEventFamily::SecurityDecision)
        }
        TraceEvent::AdminOperation(trace) => Some(admin_operation_family(trace.operation)),
        TraceEvent::ContractRejected(_) => Some(DurableAuditEventFamily::AdmissionDecision),
        TraceEvent::CatalogMutation(_) | TraceEvent::Manifest(_) => {
            Some(DurableAuditEventFamily::CatalogDecision)
        }
        TraceEvent::Wal(_)
        | TraceEvent::WalEvent(_)
        | TraceEvent::CommitVisible(_)
        | TraceEvent::RollbackDurable(_)
        | TraceEvent::RecoveryStartup(_)
        | TraceEvent::CorruptionBoundary(_) => Some(DurableAuditEventFamily::RecoveryDecision),
        TraceEvent::Audit(_) => Some(DurableAuditEventFamily::GenericAudit),
        _ => None,
    }
}

fn admin_operation_family(operation: AdminOperation) -> DurableAuditEventFamily {
    match operation {
        AdminOperation::Backup => DurableAuditEventFamily::BackupDecision,
        AdminOperation::Restore => DurableAuditEventFamily::RestoreDecision,
        AdminOperation::ForensicStart => DurableAuditEventFamily::ForensicDecision,
        AdminOperation::ClusterPromote
        | AdminOperation::FenceNode
        | AdminOperation::UpdateClusterManifest => DurableAuditEventFamily::HadrDecision,
        AdminOperation::DebugProcedure
        | AdminOperation::ReadProcedureStore
        | AdminOperation::InspectPlans
        | AdminOperation::ManageSecurity
        | AdminOperation::RotateCertificate
        | AdminOperation::RevokeCertificateIdentity => DurableAuditEventFamily::AdminDecision,
    }
}
