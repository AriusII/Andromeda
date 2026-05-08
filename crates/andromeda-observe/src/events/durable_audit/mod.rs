mod append;
mod checksum;
mod compaction;
mod decision_gate;
mod error;
mod event_mapping;
mod failure;
mod family;
mod file_sink;
mod identity;
mod journal_format;
mod journal_mutation;
mod open;
mod pending_record;
mod policy_requirement;
mod principal_binding;
mod replay;
mod replay_behavior;
mod replay_query;
mod replay_record;
mod retention;
mod sink;
mod sink_report;
mod validation;
mod wal_evidence;

pub(crate) use append::append_record;
pub(crate) use checksum::checksum64;
pub(crate) use compaction::{compact_records, compact_records_with_archive_proofs};
pub use decision_gate::{DurableAuditDecisionGate, DurableAuditVisibleDecisionProof};
pub(crate) use error::sink_failure;
pub use error::{DurableAuditSinkFailure, DurableAuditSinkResult};
pub(crate) use event_mapping::durable_audit_family;
pub use failure::DurableAuditFailureKind;
pub use family::DurableAuditEventFamily;
pub use file_sink::FileDurableAuditWalSink;
pub use identity::DurableAuditRecordIdentity;
pub(crate) use open::open_sink;
pub use pending_record::PendingDurableAuditRecord;
pub use policy_requirement::{
    DurableAuditPolicyEvidenceRequirement, classify_policy_evidence_requirement,
};
pub use principal_binding::DurableAuditPrincipalBinding;
pub(crate) use replay::{replay_records, replay_records_with_evidence};
pub use replay_behavior::DurableAuditReplayBehavior;
pub use replay_query::{
    DurableAuditReplayEvidence, DurableAuditReplayLsnRange, DurableAuditReplayQuery,
    DurableAuditReplayResult, DurableAuditReplayWindow,
};
pub use replay_record::DurableAuditReplayRecord;
pub use retention::{
    DurableAuditCompactionReport, DurableAuditPruneBlockReason, DurableAuditPruneEvidence,
    DurableAuditRetentionBoundary, DurableAuditRetentionManager, DurableAuditRetentionPolicy,
    DurableAuditWalSegmentArchiveProof,
};
pub use sink::DurableAuditWalSink;
pub use sink_report::DurableAuditSinkReport;
pub(crate) use validation::{validate_permissioned_critical_policy_binding, validate_record};
pub use wal_evidence::DurableAuditWalEvidence;
