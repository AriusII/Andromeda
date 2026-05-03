use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct EngineTimestamp(u64);

impl EngineTimestamp {
    pub const fn from_unix_millis(value: u64) -> Self {
        Self(value)
    }

    pub const fn as_unix_millis(self) -> u64 {
        self.0
    }
}

pub trait Clock {
    fn now(&self) -> EngineTimestamp;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> EngineTimestamp {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();

        EngineTimestamp::from_unix_millis(millis.min(u128::from(u64::MAX)) as u64)
    }
}
