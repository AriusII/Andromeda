use andromeda_core::InvocationId;

use crate::TraceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvocationTrace {
    pub trace_id: TraceId,
    pub invocation_id: InvocationId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MvccTrace {
    pub trace_id: TraceId,
    pub snapshot_ts: u64,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditTrace {
    pub trace_id: TraceId,
    pub actor: String,
    pub object: String,
    pub action: String,
}

impl AuditTrace {
    pub fn is_complete(&self) -> bool {
        !self.actor.trim().is_empty()
            && !self.object.trim().is_empty()
            && !self.action.trim().is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceTrace {
    pub trace_id: TraceId,
    pub memory_bytes: u64,
    pub temp_bytes: u64,
}
