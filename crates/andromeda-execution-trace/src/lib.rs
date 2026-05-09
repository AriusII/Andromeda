#![forbid(unsafe_code)]

//! Wire execution trace infrastructure.
//!
//! This module coordinates ProcedureInvocationTrace emission across all critical
//! execution pipeline phases:
//!
//! 1. **Admission Decision** - Accepted/denied with reason (permission, contract, resource)
//! 2. **Dispatch Event** - Procedure resolved, executor selected
//! 3. **Execution Start/End** - Timestamps, result cardinality
//! 4. **Error/Rollback** - Failure reason, state transition
//!
//! ## Durable Audit Guarantees
//!
//! All traces are emitted to an append-only audit ledger. The invariant is
//! enforced: **Every procedure invocation produces a durable trace with no
//! silent omissions.**
//!
//! Integration points:
//! - `InvocationRequest::validate_admission` to `AdmissionDecisionTrace`
//! - `ProcedureDispatcher::dispatch_procedure` to `DispatchEventTrace`
//! - `LocalVerticalRuntime::execute` to `ExecutionStartTrace`
//! - Commit/rollback completion to `ExecutionEndTrace` (bound to terminal LSN)
//!
//! ## Trace Correlation
//!
//! All events share a `trace_id` (from `InvocationContext` or request). This
//! enables forensic replay and recovery validation across restarts.

use andromeda_error::AndromedaResult;
use andromeda_observability::TraceId;
use andromeda_types::InvocationId;
use std::sync::Arc;

pub mod completion;
pub mod transaction_error_routing;

pub use completion::*;
pub use transaction_error_routing::*;

/// Minimal trace event type for invocation lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvocationTraceEvent {
    /// Pre-transaction admission gate decision (accept/deny).
    AdmissionDecision {
        trace_id: TraceId,
        invocation_id: InvocationId,
        accepted: bool,
        reason: String,
    },

    /// Procedure successfully resolved and executor selected.
    DispatchEvent {
        trace_id: TraceId,
        invocation_id: InvocationId,
        procedure_name: String,
        executor_kind: String,
    },

    /// Procedure execution commenced (transaction allocated).
    ExecutionStart {
        trace_id: TraceId,
        invocation_id: InvocationId,
        transaction_id: andromeda_types::TransactionId,
    },

    /// Procedure execution completed (success or rollback).
    ExecutionEnd {
        trace_id: TraceId,
        invocation_id: InvocationId,
        status: String,
        rows_affected: Option<u64>,
        result_cardinality: Option<String>,
    },

    /// Execution failed with error/rollback.
    ExecutionFailed {
        trace_id: TraceId,
        invocation_id: InvocationId,
        failure_reason: String,
        recoverable: bool,
    },

    /// Invocation was aborted because a timeout deadline was exceeded.
    ///
    /// Emitted at the QUIC stream layer for `IdleTimeout` and `OverallTimeout`,
    /// and reserved for `LockWait` once lock acquisition exposes a deadline-aware
    /// API.
    ///
    /// This event is **always terminal**: no retry occurs after `TimeoutExceeded`
    /// (`ErrorRetryability::classify(Timeout) == Persistent`).
    TimeoutExceeded {
        trace_id: TraceId,
        invocation_id: InvocationId,
        /// Describes which deadline was exceeded: "IdleTimeout", "OverallTimeout",
        /// or "LockWait".
        deadline_kind: String,
        /// Wall-clock milliseconds elapsed from stream creation to timeout detection.
        elapsed_ms: u64,
    },
}

impl InvocationTraceEvent {
    /// Extract the trace_id from this event.
    pub fn trace_id(&self) -> TraceId {
        match self {
            Self::AdmissionDecision { trace_id, .. } => *trace_id,
            Self::DispatchEvent { trace_id, .. } => *trace_id,
            Self::ExecutionStart { trace_id, .. } => *trace_id,
            Self::ExecutionEnd { trace_id, .. } => *trace_id,
            Self::ExecutionFailed { trace_id, .. } => *trace_id,
            Self::TimeoutExceeded { trace_id, .. } => *trace_id,
        }
    }

    /// Extract the invocation_id from this event.
    pub fn invocation_id(&self) -> InvocationId {
        match self {
            Self::AdmissionDecision { invocation_id, .. } => *invocation_id,
            Self::DispatchEvent { invocation_id, .. } => *invocation_id,
            Self::ExecutionStart { invocation_id, .. } => *invocation_id,
            Self::ExecutionEnd { invocation_id, .. } => *invocation_id,
            Self::ExecutionFailed { invocation_id, .. } => *invocation_id,
            Self::TimeoutExceeded { invocation_id, .. } => *invocation_id,
        }
    }
}

/// Trait for durable audit ledger that accepts trace events.
/// Implementation must guarantee append-only semantics and no silent drops.
pub trait AuditLedger: Send + Sync {
    /// Append a trace event to the durable ledger.
    /// Returns an error if the ledger is unavailable or corrupted.
    /// Never silently drops events.
    fn append_trace(&self, event: InvocationTraceEvent) -> AndromedaResult<()>;

    /// Query traces by trace_id (for forensic/debugging only).
    fn query_by_trace_id(&self, trace_id: TraceId) -> AndromedaResult<Vec<InvocationTraceEvent>>;

    /// Query traces by invocation_id (for forensic/debugging only).
    fn query_by_invocation_id(
        &self,
        invocation_id: InvocationId,
    ) -> AndromedaResult<Vec<InvocationTraceEvent>>;

    /// Count of traces successfully appended.
    fn total_appended(&self) -> u64;

    /// Count of append attempts rejected (ledger unavailable, validation error, etc.).
    fn total_rejected(&self) -> u64;
}

/// In-memory audit ledger for testing and development.
/// Production deployments must wire a persistent ledger (WAL-backed or external).
#[derive(Debug, Clone)]
pub struct InMemoryAuditLedger {
    events: Arc<std::sync::Mutex<Vec<InvocationTraceEvent>>>,
    appended_count: Arc<std::sync::atomic::AtomicU64>,
    rejected_count: Arc<std::sync::atomic::AtomicU64>,
}

impl InMemoryAuditLedger {
    /// Create a new in-memory audit ledger.
    pub fn new() -> Self {
        Self {
            events: Arc::new(std::sync::Mutex::new(Vec::new())),
            appended_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            rejected_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Snapshot of all recorded events (read-only).
    pub fn snapshot(&self) -> AndromedaResult<Vec<InvocationTraceEvent>> {
        let events = self.events.lock().map_err(|e| {
            andromeda_error::AndromedaError::new(
                andromeda_error::AndromedaErrorKind::Internal,
                format!("audit ledger lock poisoned: {}", e),
            )
        })?;
        Ok(events.clone())
    }
}

impl Default for InMemoryAuditLedger {
    fn default() -> Self {
        Self::new()
    }
}

impl AuditLedger for InMemoryAuditLedger {
    fn append_trace(&self, event: InvocationTraceEvent) -> AndromedaResult<()> {
        match self.events.lock() {
            Ok(mut guard) => {
                guard.push(event);
                self.appended_count
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            },
            Err(e) => {
                self.rejected_count
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err(andromeda_error::AndromedaError::new(
                    andromeda_error::AndromedaErrorKind::Internal,
                    format!("audit ledger lock poisoned: {}", e),
                ))
            },
        }
    }

    fn query_by_trace_id(&self, trace_id: TraceId) -> AndromedaResult<Vec<InvocationTraceEvent>> {
        let events = self.snapshot()?;
        Ok(events
            .into_iter()
            .filter(|e| match e {
                InvocationTraceEvent::AdmissionDecision { trace_id: t, .. } => t == &trace_id,
                InvocationTraceEvent::DispatchEvent { trace_id: t, .. } => t == &trace_id,
                InvocationTraceEvent::ExecutionStart { trace_id: t, .. } => t == &trace_id,
                InvocationTraceEvent::ExecutionEnd { trace_id: t, .. } => t == &trace_id,
                InvocationTraceEvent::ExecutionFailed { trace_id: t, .. } => t == &trace_id,
                InvocationTraceEvent::TimeoutExceeded { trace_id: t, .. } => t == &trace_id,
            })
            .collect())
    }

    fn query_by_invocation_id(
        &self,
        invocation_id: InvocationId,
    ) -> AndromedaResult<Vec<InvocationTraceEvent>> {
        let events = self.snapshot()?;
        Ok(events
            .into_iter()
            .filter(|e| match e {
                InvocationTraceEvent::AdmissionDecision {
                    invocation_id: i, ..
                } => i == &invocation_id,
                InvocationTraceEvent::DispatchEvent {
                    invocation_id: i, ..
                } => i == &invocation_id,
                InvocationTraceEvent::ExecutionStart {
                    invocation_id: i, ..
                } => i == &invocation_id,
                InvocationTraceEvent::ExecutionEnd {
                    invocation_id: i, ..
                } => i == &invocation_id,
                InvocationTraceEvent::ExecutionFailed {
                    invocation_id: i, ..
                } => i == &invocation_id,
                InvocationTraceEvent::TimeoutExceeded {
                    invocation_id: i, ..
                } => i == &invocation_id,
            })
            .collect())
    }

    fn total_appended(&self) -> u64 {
        self.appended_count
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    fn total_rejected(&self) -> u64 {
        self.rejected_count
            .load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_audit_ledger_appends() {
        let ledger = InMemoryAuditLedger::new();
        let trace_id = TraceId::new(1);
        let invocation_id = andromeda_types::InvocationId::new(42);

        let event = InvocationTraceEvent::AdmissionDecision {
            trace_id,
            invocation_id,
            accepted: true,
            reason: "test admission".to_string(),
        };

        let result = ledger.append_trace(event.clone());
        assert!(result.is_ok());
        assert_eq!(ledger.total_appended(), 1);
        assert_eq!(ledger.total_rejected(), 0);

        let snapshot = ledger.snapshot().expect("snapshot failed");
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0], event);
    }

    #[test]
    fn test_in_memory_audit_ledger_query_by_trace_id() {
        let ledger = InMemoryAuditLedger::new();
        let trace_id1 = TraceId::new(1);
        let trace_id2 = TraceId::new(2);
        let invocation_id = andromeda_types::InvocationId::new(42);

        let event1 = InvocationTraceEvent::AdmissionDecision {
            trace_id: trace_id1,
            invocation_id,
            accepted: true,
            reason: "test 1".to_string(),
        };

        let event2 = InvocationTraceEvent::AdmissionDecision {
            trace_id: trace_id2,
            invocation_id,
            accepted: false,
            reason: "test 2".to_string(),
        };

        ledger.append_trace(event1).expect("append 1 failed");
        ledger.append_trace(event2).expect("append 2 failed");

        let results = ledger.query_by_trace_id(trace_id1).expect("query failed");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_in_memory_audit_ledger_query_by_invocation_id() {
        let ledger = InMemoryAuditLedger::new();
        let trace_id = TraceId::new(1);
        let invocation_id1 = andromeda_types::InvocationId::new(42);
        let invocation_id2 = andromeda_types::InvocationId::new(43);

        let event1 = InvocationTraceEvent::AdmissionDecision {
            trace_id,
            invocation_id: invocation_id1,
            accepted: true,
            reason: "test 1".to_string(),
        };

        let event2 = InvocationTraceEvent::AdmissionDecision {
            trace_id,
            invocation_id: invocation_id2,
            accepted: true,
            reason: "test 2".to_string(),
        };

        ledger.append_trace(event1).expect("append 1 failed");
        ledger.append_trace(event2).expect("append 2 failed");

        let results = ledger
            .query_by_invocation_id(invocation_id1)
            .expect("query failed");
        assert_eq!(results.len(), 1);
    }
}
