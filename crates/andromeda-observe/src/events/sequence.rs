use andromeda_core::{
    AndromedaResult, CatalogObjectId, CatalogVersion, ContractHash, RequestId, SessionId,
    TransactionId,
};

use super::{
    EventEnvelope, EventId, EventSink, SecurityAuditOutcome, SecurityAuditTrace, TraceEvent,
    observe_error,
};

mod lifecycle;

use lifecycle::{ProcedureLifecycleCursor, ProcedureLifecycleStep};

/// Bounded in-memory validator for procedure lifecycle event order.
#[derive(Debug, Clone)]
pub struct InMemoryEventSequence {
    events: Vec<EventEnvelope>,
    max_events: Option<usize>,
    cursor: ProcedureLifecycleCursor,
    last_event_id: Option<EventId>,
    security_audit_seen: bool,
    request_id: Option<RequestId>,
    session_id: Option<SessionId>,
    contract_hash: Option<ContractHash>,
    catalog_version: Option<CatalogVersion>,
    catalog_object_id: Option<CatalogObjectId>,
    transaction_id: Option<TransactionId>,
    durable_lsn: Option<u64>,
}

/// Procedure lifecycle trace helper.
pub type ProcedureLifecycleTrace = InMemoryEventSequence;

impl Default for InMemoryEventSequence {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryEventSequence {
    pub const fn new() -> Self {
        Self::with_max_events(None)
    }

    pub const fn with_capacity_limit(max_events: usize) -> Self {
        Self::with_max_events(Some(max_events))
    }

    const fn with_max_events(max_events: Option<usize>) -> Self {
        Self {
            events: Vec::new(),
            max_events,
            cursor: ProcedureLifecycleCursor::Empty,
            last_event_id: None,
            security_audit_seen: false,
            request_id: None,
            session_id: None,
            contract_hash: None,
            catalog_version: None,
            catalog_object_id: None,
            transaction_id: None,
            durable_lsn: None,
        }
    }

    pub fn events(&self) -> &[EventEnvelope] {
        &self.events
    }

    pub fn into_events(self) -> Vec<EventEnvelope> {
        self.events
    }

    pub fn append(&mut self, event: EventEnvelope) -> AndromedaResult<()> {
        event.validate()?;

        if self
            .max_events
            .is_some_and(|max_events| self.events.len() >= max_events)
        {
            return Err(observe_error(
                "in-memory event sequence capacity exhausted; event was not recorded",
            ));
        }

        self.validate_event_id_order(event.event_id)?;
        self.validate_stable_procedure_correlation(&event)?;

        let step = ProcedureLifecycleStep::from_event(&event)?;
        let next = self.validate_transition(step, &event)?;

        self.apply_step(step, &event);
        self.cursor = next;
        self.last_event_id = Some(event.event_id);
        self.events.push(event);
        Ok(())
    }

    pub fn is_recovery_started(&self) -> bool {
        self.cursor == ProcedureLifecycleCursor::RecoveryStarted
    }

    pub fn has_terminal_pre_transaction_rejection(&self) -> bool {
        matches!(
            self.cursor,
            ProcedureLifecycleCursor::PreTransactionRejected
                | ProcedureLifecycleCursor::PreTransactionCompletionEmitted
        )
    }

    pub fn has_mandatory_security_audit(&self) -> bool {
        self.security_audit_seen
    }

    pub fn has_terminal_completion(&self) -> bool {
        matches!(
            self.cursor,
            ProcedureLifecycleCursor::CompletionEmitted
                | ProcedureLifecycleCursor::PreTransactionCompletionEmitted
                | ProcedureLifecycleCursor::RecoveryStarted
        )
    }

    fn validate_event_id_order(&self, event_id: EventId) -> AndromedaResult<()> {
        if self
            .last_event_id
            .is_some_and(|last_event_id| event_id <= last_event_id)
        {
            return Err(observe_error(
                "procedure lifecycle event sequence requires strictly increasing event_id values",
            ));
        }

        Ok(())
    }

    fn validate_stable_procedure_correlation(&self, event: &EventEnvelope) -> AndromedaResult<()> {
        let correlation = event.correlation;
        if !correlation.has_request_session()
            || !correlation.has_contract_catalog()
            || correlation
                .catalog_object_id
                .is_none_or(|catalog_object_id| catalog_object_id.get() == 0)
        {
            return Err(observe_error(
                "procedure lifecycle events require non-zero request_id, session_id, contract_hash, catalog_version, and catalog_object_id correlation",
            ));
        }

        Self::validate_anchor("request_id", self.request_id, correlation.request_id)?;
        Self::validate_anchor("session_id", self.session_id, correlation.session_id)?;
        Self::validate_anchor(
            "contract_hash",
            self.contract_hash,
            correlation.contract_hash,
        )?;
        Self::validate_anchor(
            "catalog_version",
            self.catalog_version,
            correlation.catalog_version,
        )?;
        Self::validate_anchor(
            "catalog_object_id",
            self.catalog_object_id,
            correlation.catalog_object_id,
        )
    }

    fn validate_anchor<T: Copy + Eq>(
        label: &str,
        expected: Option<T>,
        observed: Option<T>,
    ) -> AndromedaResult<()> {
        if let Some(expected) = expected
            && observed != Some(expected)
        {
            return Err(observe_error(format!(
                "procedure lifecycle {label} correlation must remain stable across the sequence",
            )));
        }

        Ok(())
    }

    fn validate_transition(
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

    fn apply_step(&mut self, step: ProcedureLifecycleStep, event: &EventEnvelope) {
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
            }
            ProcedureLifecycleStep::RecoveryStarted { last_durable_lsn } => {
                self.durable_lsn = Some(last_durable_lsn);
            }
            ProcedureLifecycleStep::AdmissionAccepted
            | ProcedureLifecycleStep::Authorized
            | ProcedureLifecycleStep::IoAdmitted
            | ProcedureLifecycleStep::CompletionEmitted { .. }
            | ProcedureLifecycleStep::PreTransactionRejected => {}
        }
    }
}

impl EventSink for InMemoryEventSequence {
    fn emit(&mut self, event: EventEnvelope) -> AndromedaResult<()> {
        self.append(event)
    }
}
