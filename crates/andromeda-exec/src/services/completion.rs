//! Compatibility facade for invocation completion ownership.

pub use andromeda_execution_trace::{
    CompletionAuditEvidence, CompletionAuditPolicy, CompletionEmission, CompletionJournalRecord,
    CompletionMappingService, CompletionRecoveryAmbiguity, CompletionRecoveryExpectation,
    CompletionRecoveryRecord, CompletionRecoveryReport, CompletionRecoveryStatus,
    InvocationCompletionEmitter, InvocationCompletionJournal,
    reconcile_completion_recovery_from_wal,
};
