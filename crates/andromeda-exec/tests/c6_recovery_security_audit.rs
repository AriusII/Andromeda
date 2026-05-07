//! C6: Recovery and Security Audit Evidence Projection
//!
//! This integration crate verifies that recovery startup, WAL replay, and
//! authorization decisions produce observable, traceable audit events with no
//! silent drops.
//!
//! # Audit Trace Specifications
//!
//! ## Recovery Audit Trail
//!
//! Recovery startup MUST emit a `RecoveryAuditTrace`, each WAL replay batch MUST
//! emit replay evidence, and publication decisions MUST remain correlated to the
//! same recovery trace.
//!
//! ## Security Audit Trail
//!
//! Every authorization check MUST emit `SecurityAuditTrace` evidence for allow
//! and deny paths. Admission rejection MUST leave no transaction, WAL, or local
//! runtime entry behind.
//!
//! # Invariant: No Silent Drops
//!
//! `EventEmitter` validates every envelope before insertion; failures are
//! returned to the caller and counted as observable rejects.

#[path = "c6_recovery_security_audit/emitter.rs"]
mod emitter;
#[path = "c6_recovery_security_audit/recovery.rs"]
mod recovery;
#[path = "c6_recovery_security_audit/security.rs"]
mod security;
#[path = "c6_recovery_security_audit/support.rs"]
mod support;
