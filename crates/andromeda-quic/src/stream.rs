#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamRole {
    SessionControl,
    CommandBidirectional,
    ResultUnidirectional,
    Diagnostic,
    TelemetryDatagram,
}
