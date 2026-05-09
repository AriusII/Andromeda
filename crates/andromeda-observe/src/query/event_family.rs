use andromeda_observability::{TraceEventFamily, TraceEventFamilyClassified};

use crate::TraceEvent;

impl TraceEventFamilyClassified for TraceEvent {
    fn trace_event_family(&self) -> TraceEventFamily {
        match self {
            TraceEvent::Decision(_) => TraceEventFamily::Decision,
            TraceEvent::Invocation(_) | TraceEvent::ExecutionTransition(_) => {
                TraceEventFamily::ProcedureInvocation
            },
            TraceEvent::Wal(_)
            | TraceEvent::WalEvent(_)
            | TraceEvent::CommitVisible(_)
            | TraceEvent::RollbackDurable(_)
            | TraceEvent::CorruptionBoundary(_) => TraceEventFamily::Wal,
            TraceEvent::RecoveryStartup(_) => TraceEventFamily::Recovery,
            TraceEvent::Manifest(_) | TraceEvent::CatalogMutation(_) => {
                TraceEventFamily::ManifestCatalog
            },
            TraceEvent::FrameRejection(_)
            | TraceEvent::StreamRoleRejection(_)
            | TraceEvent::Backpressure(_)
            | TraceEvent::CompletionEmitted(_)
            | TraceEvent::ContractRejected(_)
            | TraceEvent::UnsupportedVersion(_)
            | TraceEvent::SchemaLayoutDecision(_) => TraceEventFamily::Protocol,
            TraceEvent::AuthorizationDenied(_) | TraceEvent::SecurityAudit(_) => {
                TraceEventFamily::SecurityAudit
            },
            TraceEvent::AdminOperation(_) | TraceEvent::Audit(_) => TraceEventFamily::AdminAudit,
            TraceEvent::Resource(_) => TraceEventFamily::Resource,
            TraceEvent::IoPlacementDecision(_)
            | TraceEvent::PlacementAudit(_)
            | TraceEvent::IoBudgetDecision(_) => TraceEventFamily::Io,
            TraceEvent::GpuPolicyDecision(_) => TraceEventFamily::Gpu,
            TraceEvent::Mvcc(_) | TraceEvent::TransactionTransition(_) => {
                TraceEventFamily::Transaction
            },
        }
    }
}
