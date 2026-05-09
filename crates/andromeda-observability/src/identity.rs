crate::define_observability_u128_id!(EventId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventSchemaVersion(u16);

impl EventSchemaVersion {
    pub const V0: Self = Self(1);

    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u16 {
        self.0
    }

    pub const fn is_v0(self) -> bool {
        self.0 == Self::V0.0
    }
}

pub const V0_EVENT_SCHEMA_VERSION: EventSchemaVersion = EventSchemaVersion::V0;
