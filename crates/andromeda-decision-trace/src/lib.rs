#![forbid(unsafe_code)]

//! Runtime-free Andromeda DecisionTrace contracts.
//!
//! This crate owns the typed, bounded evidence shape used by optimizer,
//! statistics, plan-cache, benchmark, analytics, and operator explanation
//! paths. It does not own sinks, journals, exporters, or durable audit storage.
//!
//! Ownership constraints:
//! - DecisionTrace explains decisions after the fact. It is not storage,
//!   catalog, transaction, recovery, or security truth.
//! - Adaptive traces must carry version bindings and stale-evidence status.
//! - Benchmark output, ScenarioEvidence, analytics output, and GPU output remain
//!   advisory even when represented in a trace.
//! - Trace contracts must stay runtime-free and redaction-safe.

mod control;
mod critical;
mod error;
mod trace;
mod version;

pub use control::{AdaptiveControl, AdaptiveFeature};
pub use critical::{CriticalDecisionKind, CriticalDecisionTrace};
pub use error::DecisionTraceError;
pub use trace::{
    DECISION_EXPLANATION_MAX_BYTES, DECISION_REASON_CODE_MAX_BYTES, DecisionFamily,
    DecisionOutcome, DecisionReasonCode, DecisionTrace, DecisionTraceId, EVIDENCE_LABEL_MAX_BYTES,
    EVIDENCE_REFERENCE_LIMIT, EvidenceDigest, EvidenceLabel, TraceEvidence, VersionBinding,
};
pub use version::{DECISION_TRACE_SCHEMA_VERSION, DecisionTraceSchemaVersion};

/// Return the first eight bytes of a 32-byte digest as lowercase hex.
///
/// Decision traces use this bounded prefix for human-readable correlation
/// while preserving the full digest in typed evidence fields.
pub fn digest_prefix_hex(digest: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut out = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        out.push(char::from(HEX[(byte >> 4) as usize]));
        out.push(char::from(HEX[(byte & 0x0F) as usize]));
    }
    out
}
