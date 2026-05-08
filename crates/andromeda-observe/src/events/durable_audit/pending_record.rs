use crate::events::{EventEnvelope, observe_error};
use andromeda_error::AndromedaResult;

use super::{
    DurableAuditPrincipalBinding, DurableAuditRecordIdentity, DurableAuditReplayBehavior,
    DurableAuditRetentionBoundary, durable_audit_family, validate_record,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDurableAuditRecord {
    pub identity: DurableAuditRecordIdentity,
    pub principal_binding: DurableAuditPrincipalBinding,
    pub retention: DurableAuditRetentionBoundary,
    pub replay_behavior: DurableAuditReplayBehavior,
    pub envelope: EventEnvelope,
}

impl PendingDurableAuditRecord {
    pub fn new(
        sequence_number: u64,
        principal_binding: DurableAuditPrincipalBinding,
        retention: DurableAuditRetentionBoundary,
        replay_behavior: DurableAuditReplayBehavior,
        envelope: EventEnvelope,
    ) -> AndromedaResult<Self> {
        let family = durable_audit_family(&envelope.event).ok_or_else(|| {
            observe_error("event family is not eligible for durable audit WAL recording")
        })?;
        let record = Self {
            identity: DurableAuditRecordIdentity {
                event_id: envelope.event_id,
                trace_id: envelope.trace_id,
                family,
                sequence_number,
            },
            principal_binding,
            retention,
            replay_behavior,
            envelope,
        };
        record.validate()?;
        Ok(record)
    }

    /// Security decisions are fail-closed: validation must reject records that
    /// cannot be correlated to request/session evidence.
    pub fn validate(&self) -> AndromedaResult<()> {
        validate_record(self)
    }
}
