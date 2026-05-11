//! Cooperative kill switch for GPU job cancellation.
//!
//! [`KillSwitchHandle`] wraps an `Arc<AtomicU8>` providing a safe, cooperative
//! cancellation signal that can be shared across threads. No unsafe code is
//! required; all operations use standard atomic loads and stores.
//!
//! When the kill switch is [`Active`][KillSwitch::Active], GPU execution paths
//! short-circuit immediately. The CPU fallback is intentionally **not** invoked
//! in this case — the cancellation is total.

#![allow(clippy::module_name_repetitions)]

use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

/// Current state of the kill switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KillSwitch {
    /// The kill switch has been set; GPU execution must not proceed.
    Active(KillReason),
    /// The kill switch is not set; GPU execution may proceed.
    Inactive,
}

/// Reason the kill switch was set to [`Active`][KillSwitch::Active].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillReason {
    /// An operator explicitly requested cancellation.
    OperatorRequested,
    /// Resource budget was exceeded during a previous execution.
    BudgetExceeded,
    /// A health check detected a device or system failure.
    HealthCheckFailed,
    /// The system is shutting down.
    ShutdownInProgress,
}

impl KillReason {
    /// Encodes this reason as a non-zero `u8` for atomic storage.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::OperatorRequested => 1,
            Self::BudgetExceeded => 2,
            Self::HealthCheckFailed => 3,
            Self::ShutdownInProgress => 4,
        }
    }

    /// Decodes a `u8` to a [`KillReason`], returning `None` for unrecognised
    /// values (including zero, which represents [`KillSwitch::Inactive`]).
    #[must_use]
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            1 => Some(Self::OperatorRequested),
            2 => Some(Self::BudgetExceeded),
            3 => Some(Self::HealthCheckFailed),
            4 => Some(Self::ShutdownInProgress),
            _ => None,
        }
    }
}

/// A cloneable handle to a shared cooperative cancellation signal.
///
/// Multiple handles can share the same underlying `Arc<AtomicU8>`. Any handle
/// can cancel the shared signal; all handles observe the cancellation.
///
/// # Examples
///
/// ```rust
/// use andromeda_gpu::kill_switch::{KillReason, KillSwitch, KillSwitchHandle};
///
/// let handle = KillSwitchHandle::new();
/// assert!(!handle.is_cancelled());
///
/// handle.request_cancel(KillReason::OperatorRequested);
/// assert!(handle.is_cancelled());
/// assert_eq!(handle.current(), KillSwitch::Active(KillReason::OperatorRequested));
/// ```
#[derive(Clone)]
pub struct KillSwitchHandle {
    inner: Arc<AtomicU8>,
}

impl KillSwitchHandle {
    /// Creates a new handle in the [`Inactive`][KillSwitch::Inactive] state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AtomicU8::new(0)),
        }
    }

    /// Returns `true` if the kill switch has been set to
    /// [`Active`][KillSwitch::Active].
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.load(Ordering::Acquire) != 0
    }

    /// Sets the kill switch to [`Active`][KillSwitch::Active] with the given
    /// reason.
    ///
    /// This operation is visible to all clones of this handle.
    pub fn request_cancel(&self, reason: KillReason) {
        self.inner.store(reason.as_u8(), Ordering::Release);
    }

    /// Returns the current state of the kill switch.
    #[must_use]
    pub fn current(&self) -> KillSwitch {
        let val = self.inner.load(Ordering::Acquire);
        match KillReason::from_u8(val) {
            Some(reason) => KillSwitch::Active(reason),
            None => KillSwitch::Inactive,
        }
    }
}

impl Default for KillSwitchHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for KillSwitchHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KillSwitchHandle")
            .field("state", &self.inner.load(Ordering::Relaxed))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_handle_is_inactive() {
        let handle = KillSwitchHandle::new();
        assert!(!handle.is_cancelled());
        assert_eq!(handle.current(), KillSwitch::Inactive);
    }

    #[test]
    fn request_cancel_sets_active_state() {
        let handle = KillSwitchHandle::new();
        handle.request_cancel(KillReason::OperatorRequested);
        assert!(handle.is_cancelled());
        assert_eq!(
            handle.current(),
            KillSwitch::Active(KillReason::OperatorRequested)
        );
    }

    #[test]
    fn cloned_handle_observes_cancellation() {
        let handle_a = KillSwitchHandle::new();
        let handle_b = handle_a.clone();

        handle_a.request_cancel(KillReason::ShutdownInProgress);

        assert!(handle_b.is_cancelled());
        assert_eq!(
            handle_b.current(),
            KillSwitch::Active(KillReason::ShutdownInProgress)
        );
    }

    #[test]
    fn all_kill_reasons_round_trip_through_u8() {
        let reasons = [
            KillReason::OperatorRequested,
            KillReason::BudgetExceeded,
            KillReason::HealthCheckFailed,
            KillReason::ShutdownInProgress,
        ];
        for reason in reasons {
            let encoded = reason.as_u8();
            assert_ne!(encoded, 0, "kill reasons must encode as non-zero");
            assert_eq!(KillReason::from_u8(encoded), Some(reason));
        }
    }

    #[test]
    fn zero_decodes_to_none() {
        assert_eq!(KillReason::from_u8(0), None);
    }
}
