#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcedureRuntimeStatus {
    Committed,
    RolledBack,
    Failed,
    Aborted,
}

impl ProcedureRuntimeStatus {
    pub const VARIANT_COUNT: usize = 4;
    pub const ALL: [Self; Self::VARIANT_COUNT] = [
        Self::Committed,
        Self::RolledBack,
        Self::Failed,
        Self::Aborted,
    ];

    pub const fn as_tag(self) -> u8 {
        match self {
            Self::Committed => 0x01,
            Self::RolledBack => 0x02,
            Self::Failed => 0x03,
            Self::Aborted => 0x04,
        }
    }

    pub const fn requires_error_kind(self) -> bool {
        matches!(self, Self::RolledBack | Self::Failed | Self::Aborted)
    }

    pub const fn is_terminal(self) -> bool {
        true
    }
}
