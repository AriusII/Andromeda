use std::time::{Duration, Instant};

use super::{DeadlockResult, deadlock_error};

/// Maximum detector timeout accepted by the V0 policy.
pub const MAX_DEADLOCK_TIMEOUT: Duration = Duration::from_secs(60);

/// Default detector timeout accepted by the V0 policy.
pub const DEFAULT_DEADLOCK_TIMEOUT: Duration = Duration::from_millis(500);

/// Deterministic policy for selecting a victim after a cycle is found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlockVictimPolicy {
    /// Choose the greatest transaction start/order value.
    ///
    /// Requires [`crate::deadlock_detection::DeadlockTransactionMetadataTable`].
    /// Ties are broken by the greatest transaction identifier.
    YoungestTransactionStartOrder,
    /// Choose the greatest transaction identifier without start/order metadata.
    YoungestTransactionId,
    /// Choose the smallest transaction identifier.
    OldestTransactionId,
}

/// Bounded deadlock policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadlockPolicy {
    pub detection_timeout: Duration,
    pub victim_policy: DeadlockVictimPolicy,
}

impl DeadlockPolicy {
    pub fn new(
        detection_timeout: Duration,
        victim_policy: DeadlockVictimPolicy,
    ) -> DeadlockResult<Self> {
        let policy = Self {
            detection_timeout,
            victim_policy,
        };
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(self) -> DeadlockResult<()> {
        validate_deadlock_timeout(self.detection_timeout)
    }
}

impl Default for DeadlockPolicy {
    fn default() -> Self {
        Self {
            detection_timeout: DEFAULT_DEADLOCK_TIMEOUT,
            victim_policy: DeadlockVictimPolicy::YoungestTransactionStartOrder,
        }
    }
}

/// Storage/runtime-agnostic deadlock clock instant.
///
/// The value is a monotonic duration on an arbitrary clock timeline. It is not a
/// wall-clock timestamp and carries no durability or recovery meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeadlockClockInstant {
    elapsed_since_clock_start: Duration,
}

impl DeadlockClockInstant {
    pub const fn from_elapsed_since_clock_start(elapsed_since_clock_start: Duration) -> Self {
        Self {
            elapsed_since_clock_start,
        }
    }

    pub const fn elapsed_since_clock_start(self) -> Duration {
        self.elapsed_since_clock_start
    }

    pub fn elapsed_since(self, earlier: Self) -> Duration {
        self.elapsed_since_clock_start
            .saturating_sub(earlier.elapsed_since_clock_start)
    }
}

/// Injectable clock used by deadline-aware deadlock detection.
///
/// Implementations must be side-effect-free from the detector's point of view:
/// reading the clock may not mutate the wait-for graph, lock table, transaction
/// state, or durability state.
pub trait DeadlockClock {
    fn now(&self) -> DeadlockClockInstant;
}

/// Runtime monotonic clock adapter for deadlock detection.
///
/// This adapter reads [`Instant::now`] and never sleeps. Tests should prefer
/// [`ManualDeadlockClock`] for deterministic control.
#[derive(Debug, Clone)]
pub struct SystemDeadlockClock {
    started_at: Instant,
}

impl SystemDeadlockClock {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

impl Default for SystemDeadlockClock {
    fn default() -> Self {
        Self::new()
    }
}

impl DeadlockClock for SystemDeadlockClock {
    fn now(&self) -> DeadlockClockInstant {
        DeadlockClockInstant::from_elapsed_since_clock_start(self.started_at.elapsed())
    }
}

/// Deterministic manual clock for unit tests and runtime simulations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManualDeadlockClock {
    now: DeadlockClockInstant,
}

impl ManualDeadlockClock {
    pub const fn new() -> Self {
        Self {
            now: DeadlockClockInstant::from_elapsed_since_clock_start(Duration::ZERO),
        }
    }

    pub const fn at(elapsed_since_clock_start: Duration) -> Self {
        Self {
            now: DeadlockClockInstant::from_elapsed_since_clock_start(elapsed_since_clock_start),
        }
    }

    pub fn advance(&mut self, elapsed: Duration) -> DeadlockResult<()> {
        let Some(next) = self.now.elapsed_since_clock_start.checked_add(elapsed) else {
            return Err(deadlock_error("deadlock manual clock advance overflowed"));
        };
        self.now = DeadlockClockInstant::from_elapsed_since_clock_start(next);
        Ok(())
    }

    pub fn set(&mut self, elapsed_since_clock_start: Duration) {
        self.now = DeadlockClockInstant::from_elapsed_since_clock_start(elapsed_since_clock_start);
    }
}

impl Default for ManualDeadlockClock {
    fn default() -> Self {
        Self::new()
    }
}

impl DeadlockClock for ManualDeadlockClock {
    fn now(&self) -> DeadlockClockInstant {
        self.now
    }
}

/// Deadline for one side-effect-free deadlock detection attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadlockDetectionDeadline {
    started_at: DeadlockClockInstant,
    timeout: Duration,
}

impl DeadlockDetectionDeadline {
    pub fn new(started_at: DeadlockClockInstant, timeout: Duration) -> DeadlockResult<Self> {
        validate_deadlock_timeout(timeout)?;
        Ok(Self {
            started_at,
            timeout,
        })
    }

    pub fn from_policy<C: DeadlockClock>(
        policy: DeadlockPolicy,
        clock: &C,
    ) -> DeadlockResult<Self> {
        policy.validate()?;
        Self::new(clock.now(), policy.detection_timeout)
    }

    pub const fn started_at(self) -> DeadlockClockInstant {
        self.started_at
    }

    pub const fn timeout(self) -> Duration {
        self.timeout
    }

    pub fn elapsed_at(self, now: DeadlockClockInstant) -> Duration {
        now.elapsed_since(self.started_at)
    }

    pub fn is_expired_at(self, now: DeadlockClockInstant) -> bool {
        self.elapsed_at(now) >= self.timeout
    }

    pub fn is_expired<C: DeadlockClock>(self, clock: &C) -> bool {
        self.is_expired_at(clock.now())
    }
}

pub(super) fn validate_deadlock_timeout(detection_timeout: Duration) -> DeadlockResult<()> {
    if detection_timeout.is_zero() {
        return Err(deadlock_error(
            "deadlock detection timeout must not be zero",
        ));
    }

    if detection_timeout > MAX_DEADLOCK_TIMEOUT {
        return Err(deadlock_error(
            "deadlock detection timeout exceeds V0 bound",
        ));
    }

    Ok(())
}
