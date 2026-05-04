use crate::error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct EngineTimestamp(u64);

impl EngineTimestamp {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(u64::MAX);

    pub const fn from_unix_millis(value: u64) -> Self {
        Self(value)
    }

    pub const fn as_unix_millis(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn checked_add_millis(self, millis: u64) -> Option<Self> {
        self.0.checked_add(millis).map(Self)
    }

    pub const fn saturating_add_millis(self, millis: u64) -> Self {
        Self(self.0.saturating_add(millis))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManualClock {
    current: EngineTimestamp,
}

impl ManualClock {
    pub const fn new(current: EngineTimestamp) -> Self {
        Self { current }
    }

    pub const fn from_unix_millis(value: u64) -> Self {
        Self::new(EngineTimestamp::from_unix_millis(value))
    }

    pub fn set(&mut self, timestamp: EngineTimestamp) {
        self.current = timestamp;
    }

    pub fn advance_millis(&mut self, millis: u64) -> AndromedaResult<EngineTimestamp> {
        let next = self.current.checked_add_millis(millis).ok_or_else(|| {
            AndromedaError::new(AndromedaErrorKind::Resource, "engine timestamp overflow")
        })?;
        self.current = next;
        Ok(next)
    }
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new(EngineTimestamp::ZERO)
    }
}

impl Clock for ManualClock {
    fn now(&self) -> EngineTimestamp {
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_timestamp_exposes_deterministic_arithmetic() {
        let timestamp = EngineTimestamp::from_unix_millis(100);

        assert!(!timestamp.is_zero());
        assert_eq!(
            timestamp.checked_add_millis(23),
            Some(EngineTimestamp::from_unix_millis(123))
        );
        assert_eq!(
            EngineTimestamp::MAX.checked_add_millis(1),
            None
        );
        assert_eq!(
            EngineTimestamp::MAX.saturating_add_millis(1),
            EngineTimestamp::MAX
        );
    }

    #[test]
    fn manual_clock_is_stable_until_explicitly_advanced() {
        let mut clock = ManualClock::from_unix_millis(10);

        assert_eq!(clock.now(), EngineTimestamp::from_unix_millis(10));
        assert_eq!(
            clock.advance_millis(5).unwrap(),
            EngineTimestamp::from_unix_millis(15)
        );
        clock.set(EngineTimestamp::from_unix_millis(42));
        assert_eq!(clock.now(), EngineTimestamp::from_unix_millis(42));
    }

    #[test]
    fn manual_clock_rejects_overflow() {
        let mut clock = ManualClock::new(EngineTimestamp::MAX);

        assert_eq!(
            clock.advance_millis(1).unwrap_err().kind(),
            AndromedaErrorKind::Resource
        );
        assert_eq!(clock.now(), EngineTimestamp::MAX);
    }
}
