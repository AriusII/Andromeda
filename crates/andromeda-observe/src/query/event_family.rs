use crate::TraceEvent;

/// Closed event-family taxonomy used by the administration trace query surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TraceEventFamily {
    Decision,
    ProcedureInvocation,
    Wal,
    Recovery,
    ManifestCatalog,
    Protocol,
    SecurityAudit,
    AdminAudit,
    Resource,
    Io,
    Gpu,
    Transaction,
}

impl TraceEventFamily {
    pub fn of(event: &TraceEvent) -> Self {
        match event {
            TraceEvent::Decision(_) => Self::Decision,
            TraceEvent::Invocation(_) | TraceEvent::ExecutionTransition(_) => {
                Self::ProcedureInvocation
            }
            TraceEvent::Wal(_)
            | TraceEvent::WalEvent(_)
            | TraceEvent::CommitVisible(_)
            | TraceEvent::RollbackDurable(_)
            | TraceEvent::CorruptionBoundary(_) => Self::Wal,
            TraceEvent::RecoveryStartup(_) => Self::Recovery,
            TraceEvent::Manifest(_) | TraceEvent::CatalogMutation(_) => Self::ManifestCatalog,
            TraceEvent::FrameRejection(_)
            | TraceEvent::StreamRoleRejection(_)
            | TraceEvent::Backpressure(_)
            | TraceEvent::CompletionEmitted(_)
            | TraceEvent::ContractRejected(_)
            | TraceEvent::UnsupportedVersion(_)
            | TraceEvent::SchemaLayoutDecision(_) => Self::Protocol,
            TraceEvent::AuthorizationDenied(_) | TraceEvent::SecurityAudit(_) => {
                Self::SecurityAudit
            }
            TraceEvent::AdminOperation(_) | TraceEvent::Audit(_) => Self::AdminAudit,
            TraceEvent::Resource(_) => Self::Resource,
            TraceEvent::IoPlacementDecision(_)
            | TraceEvent::PlacementAudit(_)
            | TraceEvent::IoBudgetDecision(_) => Self::Io,
            TraceEvent::GpuPolicyDecision(_) => Self::Gpu,
            TraceEvent::Mvcc(_) | TraceEvent::TransactionTransition(_) => Self::Transaction,
        }
    }
}
