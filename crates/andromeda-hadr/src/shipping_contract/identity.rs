/// Topology role of a WAL node in the V0 single-primary design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WalNodeRole {
    /// Sole accepting node. Only role permitted to ship WAL.
    Primary,
    /// Read-only follower. Only role permitted to receive shipped WAL.
    Replica,
}

impl WalNodeRole {
    pub const fn is_primary(self) -> bool {
        matches!(self, Self::Primary)
    }

    pub const fn is_replica(self) -> bool {
        matches!(self, Self::Replica)
    }
}

/// Stable identity of a WAL participant. The numeric id is opaque; identity
/// equality is by `(id, role)` so a node cannot silently change role mid-ship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WalNodeIdentity {
    id: u64,
    role: WalNodeRole,
}

impl WalNodeIdentity {
    pub const fn new(id: u64, role: WalNodeRole) -> Self {
        Self { id, role }
    }

    pub const fn id(self) -> u64 {
        self.id
    }

    pub const fn role(self) -> WalNodeRole {
        self.role
    }
}
