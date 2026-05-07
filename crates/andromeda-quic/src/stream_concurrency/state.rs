use andromeda_core::InvocationId;
use std::time::{Duration, SystemTime};

/// Stream lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamState {
    /// Stream created, awaiting first frame.
    Created,
    /// Stream active, accepting frames.
    Active,
    /// Cancellation in progress.
    Cancelling,
    /// Stream terminal (completed or failed).
    Terminal,
}

/// Reason for stream cancellation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CancellationReason {
    /// Client explicitly requested cancellation.
    ClientRequested,
    /// Server initiated graceful shutdown.
    ServerGracefulShutdown,
    /// Stream exceeded idle timeout.
    IdleTimeout,
    /// Stream exceeded overall timeout.
    OverallTimeout,
    /// Connection lost while stream active.
    ConnectionLost,
}

/// Cancellation token for replay-safe cancellation identification.
///
/// Tokens are deterministically derived from stream metadata and are
/// monotonically increasing to support ordering guarantees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CancellationToken(u64);

impl CancellationToken {
    /// Generates a new cancellation token from an invocation ID.
    ///
    /// This function is deterministic: given the same invocation ID, it
    /// produces the same token. This ensures replay-safety.
    pub(super) fn from_invocation_id(invocation_id: InvocationId) -> Self {
        Self(invocation_id.get())
    }

    /// Returns the raw token value.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Stream metadata bound to a single concurrent procedure invocation.
#[derive(Debug)]
pub(super) struct StreamMetadata {
    state: StreamState,
    created_at: SystemTime,
    last_activity_at: SystemTime,
    cancellation_token: CancellationToken,
    cancellation_reason: Option<CancellationReason>,
}

impl StreamMetadata {
    pub(super) fn new(invocation_id: InvocationId, now: SystemTime) -> Self {
        Self {
            state: StreamState::Created,
            created_at: now,
            last_activity_at: now,
            cancellation_token: CancellationToken::from_invocation_id(invocation_id),
            cancellation_reason: None,
        }
    }

    pub(super) const fn state(&self) -> StreamState {
        self.state
    }

    pub(super) const fn cancellation_token(&self) -> CancellationToken {
        self.cancellation_token
    }

    pub(super) const fn cancellation_reason(&self) -> Option<CancellationReason> {
        self.cancellation_reason
    }

    pub(super) const fn is_terminal(&self) -> bool {
        matches!(self.state, StreamState::Terminal)
    }

    pub(super) const fn is_created(&self) -> bool {
        matches!(self.state, StreamState::Created)
    }

    pub(super) fn mark_active(&mut self, now: SystemTime) {
        self.state = StreamState::Active;
        self.last_activity_at = now;
    }

    pub(super) fn record_activity(&mut self, now: SystemTime) {
        self.last_activity_at = now;
    }

    pub(super) fn mark_cancelling(&mut self, reason: CancellationReason) {
        self.state = StreamState::Cancelling;
        self.cancellation_reason = Some(reason);
    }

    pub(super) fn mark_terminal(&mut self) {
        self.state = StreamState::Terminal;
    }

    pub(super) fn is_idle_at(&self, now: SystemTime, idle_timeout: Duration) -> bool {
        !self.is_terminal()
            && now
                .duration_since(self.last_activity_at)
                .unwrap_or_default()
                > idle_timeout
    }

    pub(super) fn activity_is_idle_at(&self, now: SystemTime, idle_timeout: Duration) -> bool {
        now.duration_since(self.last_activity_at)
            .unwrap_or_default()
            > idle_timeout
    }

    pub(super) fn is_overall_timeout_at(&self, now: SystemTime, overall_timeout: Duration) -> bool {
        !self.is_terminal()
            && now.duration_since(self.created_at).unwrap_or_default() > overall_timeout
    }
}
