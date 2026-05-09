use andromeda_error::AndromedaResult;
use andromeda_types::TransactionId;

use crate::events::{
    EventEnvelope, SecurityAuditOutcome, SecurityAuditTrace, TraceEvent, observe_error,
};

use super::{
    InMemoryEventSequence,
    lifecycle::{ProcedureLifecycleCursor, ProcedureLifecycleStep},
};

impl InMemoryEventSequence {
    pub(super) fn validate_transition(
        &self,
        step: ProcedureLifecycleStep,
        event: &EventEnvelope,
    ) -> AndromedaResult<ProcedureLifecycleCursor> {
        match (self.cursor, step) {
            (ProcedureLifecycleCursor::Empty, ProcedureLifecycleStep::AdmissionAccepted) => self
                .require_no_transaction_evidence(
                    event,
                    ProcedureLifecycleCursor::AdmissionAccepted,
                    "admission evidence",
                ),
            (ProcedureLifecycleCursor::AdmissionAccepted, ProcedureLifecycleStep::Authorized) => {
                if !matches!(
                    &event.event,
                    TraceEvent::SecurityAudit(SecurityAuditTrace {
                        outcome: SecurityAuditOutcome::Allowed,
                        ..
                    })
                ) {
                    return Err(observe_error(
                        "procedure lifecycle authorization requires a V0 security audit event",
                    ));
                }

                self.require_no_transaction_evidence(
                    event,
                    ProcedureLifecycleCursor::Authorized,
                    "authorization evidence",
                )
            }
            (ProcedureLifecycleCursor::Authorized, ProcedureLifecycleStep::IoAdmitted) => self
                .require_no_transaction_evidence(
                    event,
                    ProcedureLifecycleCursor::IoAdmitted,
                    "IO admission evidence",
                ),
            (
                ProcedureLifecycleCursor::IoAdmitted,
                ProcedureLifecycleStep::WalFlushed {
                    transaction_id,
                    durable_lsn,
                },
            ) => {
                self.require_transaction_boundary(event, transaction_id, durable_lsn)?;
                Ok(ProcedureLifecycleCursor::WalFlushed)
            }
            (
                ProcedureLifecycleCursor::WalFlushed,
                ProcedureLifecycleStep::CommitVisible {
                    transaction_id,
                    durable_lsn,
                },
            ) => {
                self.require_same_transaction_boundary(transaction_id, durable_lsn)?;
                Ok(ProcedureLifecycleCursor::CommitVisible)
            }
            (
                ProcedureLifecycleCursor::WalFlushed,
                ProcedureLifecycleStep::RollbackDurable {
                    transaction_id,
                    durable_lsn,
                },
            ) => {
                self.require_same_transaction_boundary(transaction_id, durable_lsn)?;
                Ok(ProcedureLifecycleCursor::RollbackDurable)
            }
            (
                ProcedureLifecycleCursor::CommitVisible,
                ProcedureLifecycleStep::CompletionEmitted {
                    committed: true,
                    durable_lsn: Some(durable_lsn),
                },
            ) => {
                self.require_same_durable_lsn(durable_lsn)?;
                Ok(ProcedureLifecycleCursor::CompletionEmitted)
            }
            (
                ProcedureLifecycleCursor::RollbackDurable,
                ProcedureLifecycleStep::CompletionEmitted {
                    committed: false,
                    durable_lsn: Some(durable_lsn),
                },
            ) => {
                self.require_same_durable_lsn(durable_lsn)?;
                Ok(ProcedureLifecycleCursor::CompletionEmitted)
            }
            (
                ProcedureLifecycleCursor::CompletionEmitted,
                ProcedureLifecycleStep::RecoveryStarted { last_durable_lsn },
            ) => {
                let durable_lsn = self.durable_lsn.ok_or_else(|| {
                    observe_error("recovery startup requires prior durable transaction evidence")
                })?;
                if last_durable_lsn < durable_lsn {
                    return Err(observe_error(
                        "recovery startup last durable LSN must not precede procedure durable LSN",
                    ));
                }
                Ok(ProcedureLifecycleCursor::RecoveryStarted)
            }
            (
                ProcedureLifecycleCursor::Empty | ProcedureLifecycleCursor::AdmissionAccepted,
                ProcedureLifecycleStep::PreTransactionRejected,
            ) => self
                .require_no_transaction_evidence(
                    event,
                    ProcedureLifecycleCursor::PreTransactionRejected,
                    "pre-transaction rejection evidence",
                )
                .and_then(|cursor| {
                    if matches!(&event.event, TraceEvent::AuthorizationDenied(_)) {
                        return Err(observe_error(
                            "procedure lifecycle authorization denial requires a V0 security audit event",
                        ));
                    }

                    Ok(cursor)
                }),
            (
                ProcedureLifecycleCursor::PreTransactionRejected,
                ProcedureLifecycleStep::CompletionEmitted {
                    committed: false,
                    durable_lsn: None,
                },
            ) => self.require_no_transaction_evidence(
                event,
                ProcedureLifecycleCursor::PreTransactionCompletionEmitted,
                "pre-transaction completion evidence",
            ),
            _ => Err(observe_error(format!(
                "invalid procedure lifecycle transition from {:?} using {:?}",
                self.cursor,
                event.event.kind()
            ))),
        }
    }

    fn require_no_transaction_evidence(
        &self,
        event: &EventEnvelope,
        next: ProcedureLifecycleCursor,
        label: &str,
    ) -> AndromedaResult<ProcedureLifecycleCursor> {
        if !event.correlation.has_no_transaction_evidence() {
            return Err(observe_error(format!(
                "{label} must not include transaction_id or durable_lsn correlation before WAL flush",
            )));
        }

        Ok(next)
    }

    fn require_transaction_boundary(
        &self,
        event: &EventEnvelope,
        transaction_id: TransactionId,
        durable_lsn: u64,
    ) -> AndromedaResult<()> {
        if event.correlation.transaction_id != Some(transaction_id)
            || event.correlation.durable_lsn != Some(durable_lsn)
        {
            return Err(observe_error(
                "WAL flush must introduce matching transaction_id and durable_lsn correlation",
            ));
        }

        Ok(())
    }

    fn require_same_transaction_boundary(
        &self,
        transaction_id: TransactionId,
        durable_lsn: u64,
    ) -> AndromedaResult<()> {
        if self.transaction_id != Some(transaction_id) || self.durable_lsn != Some(durable_lsn) {
            return Err(observe_error(
                "transaction terminal evidence must match the WAL flush transaction_id and durable_lsn",
            ));
        }

        Ok(())
    }

    fn require_same_durable_lsn(&self, durable_lsn: u64) -> AndromedaResult<()> {
        if self.durable_lsn != Some(durable_lsn) {
            return Err(observe_error(
                "completion evidence must match the durable LSN established by WAL flush",
            ));
        }

        Ok(())
    }

    pub(super) fn apply_step(&mut self, step: ProcedureLifecycleStep, event: &EventEnvelope) {
        if matches!(&event.event, TraceEvent::SecurityAudit(_)) {
            self.security_audit_seen = true;
        }

        self.request_id = event.correlation.request_id;
        self.session_id = event.correlation.session_id;
        self.contract_hash = event.correlation.contract_hash;
        self.catalog_version = event.correlation.catalog_version;
        self.catalog_object_id = event.correlation.catalog_object_id;

        match step {
            ProcedureLifecycleStep::WalFlushed {
                transaction_id,
                durable_lsn,
            }
            | ProcedureLifecycleStep::CommitVisible {
                transaction_id,
                durable_lsn,
            }
            | ProcedureLifecycleStep::RollbackDurable {
                transaction_id,
                durable_lsn,
            } => {
                self.transaction_id = Some(transaction_id);
                self.durable_lsn = Some(durable_lsn);
            },
            ProcedureLifecycleStep::RecoveryStarted { last_durable_lsn } => {
                self.durable_lsn = Some(last_durable_lsn);
            },
            ProcedureLifecycleStep::AdmissionAccepted
            | ProcedureLifecycleStep::Authorized
            | ProcedureLifecycleStep::IoAdmitted
            | ProcedureLifecycleStep::CompletionEmitted { .. }
            | ProcedureLifecycleStep::PreTransactionRejected => {},
        }
    }
}
