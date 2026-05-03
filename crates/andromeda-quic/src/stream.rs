#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameFamily {
    SessionControl,
    ContractControl,
    RpcCommand,
    RpcResultStream,
    Diagnostic,
    Telemetry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamRole {
    SessionControl,
    CommandBidirectional,
    ResultUnidirectional,
    Diagnostic,
    TelemetryDatagram,
}

impl StreamRole {
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
