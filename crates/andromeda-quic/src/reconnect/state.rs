/// Client connection state used by the reconnection contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectState {
    /// Requests can be admitted on the current connection.
    Connected,
    /// Existing streams may drain, but new request dispatch is blocked.
    Draining,
    /// No connection is currently usable; reconnect admission may retry.
    Disconnected,
    /// A reconnect loop is active at the recorded attempt number.
    Reconnecting { attempt: u32 },
    /// Reconnect is terminal for this connection lifecycle.
    Failed,
}

impl ReconnectState {
    pub const fn allows_new_requests(self) -> bool {
        matches!(self, Self::Connected)
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Failed)
    }

    /// Returns the active reconnect attempt for telemetry and tests.
    pub const fn reconnect_attempt(self) -> Option<u32> {
        match self {
            Self::Reconnecting { attempt } => Some(attempt),
            _ => None,
        }
    }
}
