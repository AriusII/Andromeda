mod append;
mod checksum;
mod compaction;
mod decision_gate;
mod file_sink;
mod journal_format;
mod journal_mutation;
mod open;
mod replay;
mod sink;

pub(crate) use crate::durable_audit::sink_failure;
pub(crate) use crate::{
    DurableAuditAppendRecord, DurableAuditCompactionReport, DurableAuditEventFamily,
    DurableAuditFailureKind, DurableAuditPolicyEvidenceRequirement, DurableAuditPrincipalBinding,
    DurableAuditRecordIdentity, DurableAuditReplayBehavior, DurableAuditReplayEvidence,
    DurableAuditReplayQuery, DurableAuditReplayRecord, DurableAuditReplayResult,
    DurableAuditReplayWindow, DurableAuditRetentionBoundary, DurableAuditRetentionManager,
    DurableAuditRetentionPolicy, DurableAuditSinkFailure, DurableAuditSinkReport,
    DurableAuditSinkResult, DurableAuditWalEvidence, DurableAuditWalSegmentArchiveProof,
    classify_policy_evidence_requirement,
};
pub(crate) use append::append_record;
pub(crate) use checksum::checksum64;
pub(crate) use compaction::{compact_records, compact_records_with_archive_proofs};
pub use decision_gate::{DurableAuditDecisionGate, DurableAuditVisibleDecisionProof};
pub use file_sink::FileDurableAuditWalSink;
pub(crate) use open::open_sink;
pub(crate) use replay::{replay_records, replay_records_with_evidence};
pub use sink::DurableAuditWalSink;
