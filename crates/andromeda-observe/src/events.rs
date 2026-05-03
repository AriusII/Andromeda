use andromeda_core::{InvocationId, TransactionId};

use crate::TraceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CriticalDecisionKind {
    ContractValidation,
    PlanSelection,
    TransactionCommit,
    WalFlush,
    MvccVisibility,
    RecoveryStartup,
    SecurityAuthorization,
    ResourceGovernance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionTrace {
    pub trace_id: TraceId,
    pub decision: CriticalDecisionKind,
    pub reason: String,
}

impl DecisionTrace {
    pub fn has_explanation(&self) -> bool {
        !self.reason.trim().is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvocationTrace {
    pub trace_id: TraceId,
    pub invocation_id: InvocationId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalTrace {
    pub trace_id: TraceId,
    pub transaction_id: Option<TransactionId>,
    pub durable_lsn: u64,
}

impl WalTrace {
    pub const fn proves_durable_boundary(self) -> bool {
        self.durable_lsn != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MvccTrace {
    pub trace_id: TraceId,
    pub snapshot_ts: u64,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditTrace {
    pub trace_id: TraceId,
    pub actor: String,
    pub object: String,
    pub action: String,
}

impl AuditTrace {
    pub fn is_complete(&self) -> bool {
        !self.actor.trim().is_empty()
            && !self.object.trim().is_empty()
            && !self.action.trim().is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceTrace {
    pub trace_id: TraceId,
    pub memory_bytes: u64,
    pub temp_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceEvent {
    Decision(DecisionTrace),
    Invocation(InvocationTrace),
    Wal(WalTrace),
    Mvcc(MvccTrace),
    Audit(AuditTrace),
    Resource(ResourceTrace),
}

impl TraceEvent {
    pub fn trace_id(&self) -> TraceId {
        match self {
            Self::Decision(trace) => trace.trace_id,
            Self::Invocation(trace) => trace.trace_id,
            Self::Wal(trace) => trace.trace_id,
            Self::Mvcc(trace) => trace.trace_id,
            Self::Audit(trace) => trace.trace_id,
            Self::Resource(trace) => trace.trace_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn critical_decisions_need_explanations() {
        let trace = DecisionTrace {
            trace_id: TraceId::new(1),
            decision: CriticalDecisionKind::ContractValidation,
            reason: "ContractHash matched manifest".to_string(),
        };

        assert!(trace.has_explanation());
    }

    #[test]
    fn wal_trace_proves_nonzero_durable_boundary() {
        let trace = WalTrace {
            trace_id: TraceId::new(1),
            transaction_id: Some(TransactionId::new(7)),
            durable_lsn: 42,
        };

        assert!(trace.proves_durable_boundary());
    }
}
