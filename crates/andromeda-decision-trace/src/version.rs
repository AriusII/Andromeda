/// Schema version for runtime-free DecisionTrace contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DecisionTraceSchemaVersion(u16);

impl DecisionTraceSchemaVersion {
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

pub const DECISION_TRACE_SCHEMA_VERSION: DecisionTraceSchemaVersion =
    DecisionTraceSchemaVersion::new(0);
