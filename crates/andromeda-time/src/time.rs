//! Clock abstraction and timestamp types.
//!
//! This module provides clock abstractions for deterministic time management
//! in distributed systems and testing.
//! Clock readings are observations only; durable owners must bind timestamps to
//! their own WAL, recovery, catalog, or audit evidence before treating them as
//! durable truth.
//!
//! ## Timestamps
//!
//! `EngineTimestamp` represents time as milliseconds since Unix epoch (u64).
//! It supports:
//! - Checked and saturating arithmetic
//! - Comparison and ordering
//! - Conversion to/from u64
//! - Explicit little-endian Unix-millisecond byte encoding
//!
//! The byte encoding is exactly eight bytes: an unsigned `u64` millisecond value
//! in little-endian order. Do not persist or transmit Rust native struct layout.
//!
//! ## Clocks
//!
//! The `Clock` trait abstracts time sources for:
//! - **SystemClock**: Real time via `std::time::SystemTime`
//! - **ManualClock**: Controllable time for testing (deterministic execution)
//!
//! Injection of clock implementations allows:
//! - Deterministic reproducibility
//! - Fast test execution (no real delays)
//! - Replication of specific timing scenarios
//!
//! This crate intentionally does not define deadline or `Instant` policy.
//! Monotonic timeout, retry, lock-wait, and scheduler behavior belong to the
//! subsystem that owns those decisions.
//!
//! ## Usage
//!
//! ```ignore
//! use andromeda_time::{Clock, SystemClock, ManualClock, EngineTimestamp};
//!
//! // Production: use real time
//! let clock = SystemClock;
//! let now = clock.now();
//!
//! // Testing: use controllable time
//! let mut clock = ManualClock::default();
//! clock.advance_millis(1000);
//! let later = clock.now();
//! ```

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::time::{SystemTime, UNIX_EPOCH};

/// Engine timestamp represented as unsigned milliseconds since the Unix epoch.
///
/// The value is a typed domain primitive, not a durability authority. A
/// timestamp read from [`SystemClock`] becomes durable only when an owning
/// subsystem records it through that subsystem's explicit WAL, recovery,
/// catalog, or audit evidence.
///
/// When this value must cross a persisted or network boundary, use
/// [`Self::to_unix_millis_le_bytes`] and [`Self::try_from_unix_millis_le_slice`]
/// so the width and byte order stay explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct EngineTimestamp(u64);

impl EngineTimestamp {
    /// Width of the canonical Unix-millisecond little-endian byte encoding.
    pub const UNIX_MILLIS_LE_BYTE_LEN: usize = 8;

    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(u64::MAX);

    /// Builds a timestamp from an unsigned Unix epoch millisecond value.
    pub const fn from_unix_millis(value: u64) -> Self {
        Self(value)
    }

    /// Returns the unsigned Unix epoch millisecond value.
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

    /// Encodes the Unix-millisecond value as exactly eight little-endian bytes.
    ///
    /// This is a value codec only. It does not make a wall-clock observation
    /// durable and must not replace the owning subsystem's WAL, recovery,
    /// catalog, or audit evidence.
    pub const fn to_unix_millis_le_bytes(self) -> [u8; Self::UNIX_MILLIS_LE_BYTE_LEN] {
        self.0.to_le_bytes()
    }

    /// Decodes exactly eight little-endian Unix-millisecond bytes.
    pub const fn from_unix_millis_le_bytes(bytes: [u8; Self::UNIX_MILLIS_LE_BYTE_LEN]) -> Self {
        Self(u64::from_le_bytes(bytes))
    }

    /// Validates and decodes a little-endian Unix-millisecond byte slice.
    ///
    /// The slice must be exactly [`Self::UNIX_MILLIS_LE_BYTE_LEN`] bytes.
    /// Short, long, or implicitly sized encodings are rejected so persisted and
    /// network owners cannot accidentally depend on native layout or variable
    /// width timestamp encodings.
    pub fn try_from_unix_millis_le_slice(bytes: &[u8]) -> AndromedaResult<Self> {
        let fixed = <[u8; Self::UNIX_MILLIS_LE_BYTE_LEN]>::try_from(bytes).map_err(|_| {
            AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "engine timestamp encoding must be exactly 8 little-endian Unix-millis bytes",
            )
        })?;

        Ok(Self::from_unix_millis_le_bytes(fixed))
    }
}

/// Source of engine timestamp observations.
///
/// Implementations provide typed timestamp observations only. Callers must not
/// treat a clock implementation, including [`SystemClock`], as durable truth for
/// commit visibility, recovery, catalog publication, or security decisions.
pub trait Clock {
    fn now(&self) -> EngineTimestamp;
}

/// Wall-clock adapter backed by [`SystemTime`].
///
/// `SystemClock` is appropriate at runtime boundaries that may observe current
/// wall-clock time. It is not a durability source; durable owners must record
/// timestamps through their own explicit evidence.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> EngineTimestamp {
        let millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_millis(),
            Err(_) => 0,
        };

        EngineTimestamp::from_unix_millis(millis.min(u128::from(u64::MAX)) as u64)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManualClock {
    current: EngineTimestamp,
}

impl ManualClock {
    /// Creates a deterministic manual clock at `current`.
    pub const fn new(current: EngineTimestamp) -> Self {
        Self { current }
    }

    /// Creates a deterministic manual clock from Unix epoch milliseconds.
    pub const fn from_unix_millis(value: u64) -> Self {
        Self::new(EngineTimestamp::from_unix_millis(value))
    }

    /// Sets the manual clock to an explicit timestamp.
    pub fn set(&mut self, timestamp: EngineTimestamp) {
        self.current = timestamp;
    }

    /// Advances the manual clock by a checked millisecond delta.
    ///
    /// If the addition overflows, the clock remains unchanged.
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

        assert_eq!(timestamp.as_unix_millis(), 100);
        assert!(!timestamp.is_zero());
        assert_eq!(
            timestamp.checked_add_millis(23),
            Some(EngineTimestamp::from_unix_millis(123))
        );
        assert_eq!(EngineTimestamp::MAX.checked_add_millis(1), None);
        assert_eq!(
            EngineTimestamp::MAX.saturating_add_millis(1),
            EngineTimestamp::MAX
        );
    }

    #[test]
    fn engine_timestamp_exposes_explicit_unix_millis_encoding() {
        let value = 0x0102_0304_0506_0708_u64;
        let timestamp = EngineTimestamp::from_unix_millis(value);
        let encoded = timestamp.to_unix_millis_le_bytes();

        assert_eq!(EngineTimestamp::UNIX_MILLIS_LE_BYTE_LEN, 8);
        assert_eq!(encoded, value.to_le_bytes());
        assert_ne!(encoded, value.to_be_bytes());
        assert_eq!(
            EngineTimestamp::from_unix_millis_le_bytes(encoded),
            timestamp
        );
        assert_eq!(
            EngineTimestamp::try_from_unix_millis_le_slice(&encoded),
            Ok(timestamp)
        );
    }

    #[test]
    fn engine_timestamp_rejects_implicit_or_trailing_byte_encodings() {
        let too_short = [0_u8; EngineTimestamp::UNIX_MILLIS_LE_BYTE_LEN - 1];
        let too_long = [0_u8; EngineTimestamp::UNIX_MILLIS_LE_BYTE_LEN + 1];

        for bytes in [&[][..], &too_short[..], &too_long[..]] {
            let Err(error) = EngineTimestamp::try_from_unix_millis_le_slice(bytes) else {
                panic!("invalid timestamp byte width should be rejected");
            };
            assert_eq!(error.kind(), AndromedaErrorKind::Protocol);
        }
    }

    #[test]
    fn manual_clock_is_stable_until_explicitly_advanced() {
        let mut clock = ManualClock::from_unix_millis(10);

        assert_eq!(clock.now(), EngineTimestamp::from_unix_millis(10));
        assert_eq!(
            clock.advance_millis(5),
            Ok(EngineTimestamp::from_unix_millis(15))
        );
        clock.set(EngineTimestamp::from_unix_millis(42));
        assert_eq!(clock.now(), EngineTimestamp::from_unix_millis(42));
    }

    #[test]
    fn manual_clock_rejects_overflow() {
        let mut clock = ManualClock::new(EngineTimestamp::MAX);

        assert!(matches!(
            clock.advance_millis(1),
            Err(error) if error.kind() == AndromedaErrorKind::Resource
        ));
        assert_eq!(clock.now(), EngineTimestamp::MAX);
    }
}
