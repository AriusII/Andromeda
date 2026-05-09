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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_trace_types_preserve_typed_ids_and_evidence() {
        let trace_id = TraceId::new(42);
        let invocation = InvocationTrace {
            trace_id,
            invocation_id: InvocationId::new(7),
        };
        let mvcc = MvccTrace {
            trace_id,
            snapshot_ts: 9,
            visible: true,
        };
        let resource = ResourceTrace {
            trace_id,
            memory_bytes: 1024,
            temp_bytes: 2048,
        };

        assert_eq!(invocation.trace_id, trace_id);
        assert_eq!(invocation.invocation_id.get(), 7);
        assert!(mvcc.visible);
        assert_eq!(mvcc.snapshot_ts, 9);
        assert_eq!(resource.memory_bytes, 1024);
        assert_eq!(resource.temp_bytes, 2048);
    }
}
