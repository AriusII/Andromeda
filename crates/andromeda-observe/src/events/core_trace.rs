use andromeda_types::InvocationId;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceTrace {
    pub trace_id: TraceId,
    pub memory_bytes: u64,
    pub temp_bytes: u64,
}
