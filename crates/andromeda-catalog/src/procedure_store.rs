//! Procedure Store contract registry and evidence boundary.
//!
//! The Procedure Store is the canonical, in-memory registry of executable
//! procedure identities and the contract metadata required to invoke them.
//! It is intentionally **not** an opaque runtime cache: every entry is
//! materialized from a validated [`ProcedureContract`](crate::ProcedureContract)
//! (or constructed from the same typed building blocks) and the store exposes
//! only typed accessors. There is no ad hoc text query surface (no SQL, no
//! command string); callers must address procedures by
//! [`ProcedureId`](andromeda_core::ProcedureId) or
//! [`QualifiedName`](crate::QualifiedName).
//!
//! The store also accepts [`InvocationDecisionRecord`] values, allowing every
//! invocation that touches a registered procedure to be associated with a
//! [`DecisionTrace`](andromeda_observe::DecisionTrace) for downstream audit.
//! It also stores terminal [`InvocationRuntimeRecord`] evidence: duration,
//! row/read/write counters, WAL/temp/spill bytes, optional decision-trace
//! correlation, plan class/key/id, terminal status, error kind, deterministic
//! runtime-record id, and the full procedure binding tuple.
//! The store does not emit traces itself; it is a passive evidence sink that
//! enforces the binding between evidence and the contract metadata that
//! authorized the invocation.

mod decision;
mod entry;
mod evidence_role;
mod registration;
mod runtime;
mod store;

pub use self::decision::InvocationDecisionRecord;
pub use self::entry::ProcedureStoreEntry;
pub use self::evidence_role::ProcedureStoreEvidenceRole;
pub use self::registration::ProcedureRegistration;
pub use self::runtime::{
    InvocationRuntimeRecord, InvocationRuntimeRecordOutcome, ProcedureRuntimeCounters,
    ProcedureRuntimePlanId, ProcedureRuntimeRecordId, ProcedureRuntimeStatus,
};
pub use self::store::{PROCEDURE_FEEDBACK_CAPACITY_PER_PROCEDURE, ProcedureStore};

#[cfg(test)]
mod tests;
