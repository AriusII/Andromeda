/// Client connection state used by the reconnection contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectState {
    Connected,
    Draining,
    Disconnected,
    Reconnecting { attempt: u32 },
    Failed,
}

impl ReconnectState {
    pub const fn allows_new_requests(self) -> bool {
        matches!(self, Self::Connected)
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Failed)
    }
}
