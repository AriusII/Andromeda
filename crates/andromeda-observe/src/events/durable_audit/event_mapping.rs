use super::DurableAuditEventFamily;
use crate::events::TraceEvent;

pub fn durable_audit_family(event: &TraceEvent) -> Option<DurableAuditEventFamily> {
    match event {
        TraceEvent::SecurityAudit(_) | TraceEvent::AuthorizationDenied(_) => {
            Some(DurableAuditEventFamily::SecurityDecision)
        }
        TraceEvent::AdminOperation(_) => Some(DurableAuditEventFamily::AdminDecision),
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
