use andromeda_core::{AndromedaResult, TransactionId};

use crate::events::{
    CriticalDecisionKind, EventEnvelope, SecurityAuditOutcome, SecurityAuditTrace, TraceEvent,
    WalOperation, observe_error,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProcedureLifecycleCursor {
    Empty,
    AdmissionAccepted,
    Authorized,
    IoAdmitted,
    WalFlushed,
    CommitVisible,
    RollbackDurable,
    CompletionEmitted,
    RecoveryStarted,
    PreTransactionRejected,
    PreTransactionCompletionEmitted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProcedureLifecycleStep {
    AdmissionAccepted,
    Authorized,
    IoAdmitted,
    WalFlushed {
        transaction_id: TransactionId,
        durable_lsn: u64,
    },
    CommitVisible {
        transaction_id: TransactionId,
        durable_lsn: u64,
    },
    RollbackDurable {
        transaction_id: TransactionId,
        durable_lsn: u64,
    },
    CompletionEmitted {
        committed: bool,
        durable_lsn: Option<u64>,
    },
    RecoveryStarted {
        last_durable_lsn: u64,
    },
    PreTransactionRejected,
}

impl ProcedureLifecycleStep {
    pub(super) fn from_event(event: &EventEnvelope) -> AndromedaResult<Self> {
        match &event.event {
            TraceEvent::Decision(trace)
                if trace.decision == CriticalDecisionKind::ContractValidation =>
            {
                Ok(Self::AdmissionAccepted)
            }
            TraceEvent::Decision(trace)
                if trace.decision == CriticalDecisionKind::SecurityAuthorization =>
            {
                Ok(Self::Authorized)
            }
            TraceEvent::SecurityAudit(trace) if trace.outcome == SecurityAuditOutcome::Allowed => {
                Ok(Self::Authorized)
            }
            TraceEvent::Decision(trace)
                if matches!(
                    trace.decision,
                    CriticalDecisionKind::IoBudgetValidation
                        | CriticalDecisionKind::IoPlacementDecision
                ) =>
            {
                Ok(Self::IoAdmitted)
            }
            TraceEvent::IoBudgetDecision(trace) if trace.accepted => Ok(Self::IoAdmitted),
            TraceEvent::IoPlacementDecision(trace) if trace.accepted => Ok(Self::IoAdmitted),
            TraceEvent::WalEvent(trace) if trace.operation == WalOperation::Flush => {
                let transaction_id = trace.transaction_id.ok_or_else(|| {
                    observe_error("procedure lifecycle WAL flush requires transaction_id evidence")
                })?;
                let durable_lsn = trace.durable_lsn.ok_or_else(|| {
                    observe_error("procedure lifecycle WAL flush requires durable_lsn evidence")
                })?;
                Ok(Self::WalFlushed {
                    transaction_id,
                    durable_lsn,
                })
            }
            TraceEvent::CommitVisible(trace) => Ok(Self::CommitVisible {
                transaction_id: trace.transaction_id,
                durable_lsn: trace.durable_commit_lsn,
            }),
            TraceEvent::RollbackDurable(trace) => Ok(Self::RollbackDurable {
                transaction_id: trace.transaction_id,
                durable_lsn: trace.durable_rollback_lsn,
            }),
            TraceEvent::CompletionEmitted(trace) => Ok(Self::CompletionEmitted {
                committed: trace.committed,
                durable_lsn: trace.durable_lsn,
            }),
            TraceEvent::RecoveryStartup(trace) => Ok(Self::RecoveryStarted {
                last_durable_lsn: trace.last_durable_lsn,
            }),
            TraceEvent::ContractRejected(_)
            | TraceEvent::AuthorizationDenied(_)
            | TraceEvent::SecurityAudit(SecurityAuditTrace {
                outcome: SecurityAuditOutcome::Denied,
                ..
            }) => Ok(Self::PreTransactionRejected),
            _ => Err(observe_error(
                "event is not accepted as procedure lifecycle sequence evidence",
            )),
        }
    }
}
