#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DurableAuditReplayBehavior {
    /// Replay is forensic-only and must not re-authorize or re-execute.
    ForensicOnly,
    RebuildDecisionIndex,
    CorruptionBoundary,
}
