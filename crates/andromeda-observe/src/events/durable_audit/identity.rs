use andromeda_error::AndromedaResult;

use crate::TraceId;
use crate::events::EventId;

use super::DurableAuditEventFamily;
use crate::events::observe_error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DurableAuditRecordIdentity {
    pub event_id: EventId,
    pub trace_id: TraceId,
    pub family: DurableAuditEventFamily,
    pub sequence_number: u64,
}

impl DurableAuditRecordIdentity {
    pub fn validate(self) -> AndromedaResult<()> {
        if self.event_id.is_zero() {
            return Err(observe_error(
                "durable audit record identity requires non-zero event_id",
            ));
        }
        if self.trace_id.is_zero() {
            return Err(observe_error(
                "durable audit record identity requires non-zero trace_id",
            ));
        }
        if self.sequence_number == 0 {
            return Err(observe_error(
                "durable audit record identity requires non-zero sequence_number",
            ));
        }
        Ok(())
    }
}
