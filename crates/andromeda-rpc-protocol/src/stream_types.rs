//! Stream role and family definitions.
//!
//! This module defines stream roles and frame families that separate transport surfaces
//! and enforce RPC dispatch boundaries.

/// Frame family for categorizing frame types by protocol purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameFamily {
    /// Session control frames (Hello, Auth).
    SessionControl,
    /// Contract negotiation frames.
    ContractControl,
    /// RPC command frames.
    RpcCommand,
    /// RPC result stream frames (Metadata, Batch, Completion).
    RpcResultStream,
    /// Diagnostic and error frames.
    Diagnostic,
    /// Telemetry and observability frames.
    Telemetry,
}

/// Stream role for separating transport surfaces and RPC dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamRole {
    /// Bidirectional stream for session control.
    SessionControl,
    /// Bidirectional stream for command RPC.
    CommandBidirectional,
    /// Unidirectional stream for result RPC.
    ResultUnidirectional,
    /// Diagnostic stream for errors.
    Diagnostic,
    /// Datagram for telemetry signals.
    TelemetryDatagram,
}

impl StreamRole {
    /// Returns true if this stream role permits a frame family.
    pub const fn permits_family(self, family: FrameFamily) -> bool {
        matches!(
            (self, family),
            (Self::SessionControl, FrameFamily::SessionControl)
                | (Self::CommandBidirectional, FrameFamily::ContractControl)
                | (Self::CommandBidirectional, FrameFamily::RpcCommand)
                | (Self::ResultUnidirectional, FrameFamily::RpcResultStream)
                | (Self::Diagnostic, FrameFamily::Diagnostic)
                | (Self::TelemetryDatagram, FrameFamily::Telemetry)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_role_enforces_frame_family_separation() {
        assert!(StreamRole::CommandBidirectional.permits_family(FrameFamily::RpcCommand));
        assert!(!StreamRole::ResultUnidirectional.permits_family(FrameFamily::RpcCommand));
    }
}
