//! Audit trace contracts.
//!
//! This crate owns typed audit evidence for review and forensic correlation.
//! It does not own durable journal storage, authorization truth, or recovery
//! truth.

#![forbid(unsafe_code)]

mod admin;
mod admission;
mod backup;
mod core;
mod durable_audit;
mod hadr;
mod helpers;
mod identity;
mod permission_audit_emitter;
mod scope;
mod security;

pub use admin::AdminOperationTrace;
pub use admission::{
    AdmissionAuditEvent, AdmissionDecisionKind, AffectedPrincipal, BackpressureReason,
    ContractValidationResult, ProcedureId, SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_ID,
    SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_VERSION, SecurityAdmissionAuditEventV0,
};
pub use backup::{BackupAuditEvent, BackupAuditTrace, BackupId, RecoveryStage, RestoreCompletion};
pub use core::AuditTrace;
pub use durable_audit::{
    DurableAuditCompactionReport, DurableAuditEventFamily, DurableAuditFailureKind,
    DurableAuditPolicyEvidenceRequirement, DurableAuditPrincipalBinding,
    DurableAuditPruneBlockReason, DurableAuditPruneEvidence, DurableAuditRecordIdentity,
    DurableAuditReplayBehavior, DurableAuditReplayEvidence, DurableAuditReplayLsnRange,
    DurableAuditReplayQuery, DurableAuditReplayRecord, DurableAuditReplayResult,
    DurableAuditReplayWindow, DurableAuditRetentionBoundary, DurableAuditRetentionManager,
    DurableAuditRetentionPolicy, DurableAuditSinkReport, DurableAuditWalEvidence,
    DurableAuditWalSegmentArchiveProof, classify_policy_evidence_requirement,
};
pub use hadr::{
    FencingDecision, FencingEvent, FencingPolicy, HadrAuditEvent, HadrAuditTrace,
    PromotionCompletion, PromotionEligibility, QuorumRole, ReplicaHealthState,
};
pub use identity::{
    CertificateIdentity, SecurityPolicyVersionEvidence, UserPrincipal, UserPrincipalKind,
};
pub use permission_audit_emitter::{
    AuditEmissionEventFamily, AuditEmissionEvidence, AuditEmissionKind, AuditEmissionOutcome,
    AuditEmissionPolicy, AuditEmissionReplayBehavior, AuditEmissionRetentionBoundary,
    AuditSinkAvailability, AuditSinkDurabilityEvidence, AuditSinkDurabilityReport,
    DenialAuditReason, NoOpPermissionAuditEmitter, PermissionAuditDecisionTrace,
    PermissionAuditEmitter, PermissionAuditEvent, PermissionAuditEvidence, PermissionDecisionAudit,
    audit_emission_error, audit_text_contains_sensitive_marker, redact_audit_reason,
};
pub use scope::{AdminOperation, Permission, PermissionFamily, SurfaceScope};
pub use security::{SecurityAuditDenialReason, SecurityAuditOutcome, SecurityAuditTrace};

pub use andromeda_observability::{EventSchemaVersion, TraceId};

use helpers::{
    audit_error as observe_error, contains_sensitive_marker, non_empty_evidence, non_empty_reason,
};

#[cfg(test)]
mod tests;
