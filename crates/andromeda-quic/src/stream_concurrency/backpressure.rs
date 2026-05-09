use andromeda_types::RequestId;

use crate::BackpressureReason;

/// Backpressure request from server to client.
///
/// Indicates that the server is under resource pressure and cannot accept
/// new stream creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackpressureRequest {
    /// Server-reported reason for backpressure.
    pub reason: BackpressureReason,
    /// Recommended delay before retry in milliseconds.
    pub retry_after_millis: u64,
    /// Optional request ID if backpressure is tied to a specific request.
    pub request_id: Option<RequestId>,
}

pub(super) fn capacity_backpressure() -> BackpressureRequest {
    BackpressureRequest {
        reason: BackpressureReason::ExecutionQueueSaturated,
        retry_after_millis: 100,
        request_id: None,
    }
}
