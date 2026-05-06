use andromeda_core::AndromedaResult;

use crate::events::observe_error;

use super::{
    DurableAuditRecordIdentity, DurableAuditReplayBehavior, DurableAuditRetentionBoundary,
    DurableAuditWalEvidence,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableAuditSinkReport {
    pub identity: DurableAuditRecordIdentity,
    pub evidence: DurableAuditWalEvidence,
    pub replay_behavior: DurableAuditReplayBehavior,
    pub retention: DurableAuditRetentionBoundary,
}

impl DurableAuditSinkReport {
    pub fn validate(self) -> AndromedaResult<()> {
        self.identity.validate()?;
        if !self.evidence.proves_durable() {
            return Err(observe_error(
                "durable audit sink report requires non-zero WAL LSN/checksum evidence",
            ));
        }
        Ok(())
    }
}
