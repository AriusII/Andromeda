use andromeda_error::AndromedaResult;
use andromeda_types::{
    CatalogObjectId, CatalogVersion, ContractHash, RequestId, SessionId, TransactionId,
};

use super::{EventEnvelope, EventId, EventSink, observe_error};

mod correlation;
mod lifecycle;
mod transitions;

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
}

impl EventSink for InMemoryEventSequence {
    fn emit(&mut self, event: EventEnvelope) -> AndromedaResult<()> {
        self.append(event)
    }
}
