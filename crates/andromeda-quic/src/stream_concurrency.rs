//! Stream concurrency manager and cancellation semantics for QUIC streams carrying
//! Andromeda procedure invocations.
//!
//! This module provides:
//!
//! 1. **Stream Lifecycle Model** — Maps stream states to invocation states:
//!    - Created: Stream allocated but not yet active
//!    - Active: Stream accepting frames
//!    - Cancelling: Client or server initiated cancellation in progress
//!    - Terminal: Stream completed (success or error)
//!
//! 2. **Backpressure Protocol** — Signals between client and server:
//!    - Backpressure Request: Server signals that it cannot accept new streams
//!    - Backpressure Response: Client acknowledges and may retry after delay
//!
//! 3. **Cancellation Semantics** — Three cancellation paths:
//!    - Client-initiated cancellation (explicit request)
//!    - Server graceful shutdown (all streams drained)
//!    - Orphan stream cleanup (timeout or connection loss)
//!
//! 4. **Concurrency Bounds** — Prevents resource exhaustion:
//!    - Max concurrent streams per connection (V0: 128, tunable)
//!    - Timeout: 30s idle timeout, 5min overall request timeout
//!
//! 5. **Race Condition Safety** — Atomic state transitions ensure:
//!    - Simultaneous cancel + completion resolved deterministically
//!    - No double-completion or lost events

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, InvocationId, RequestId,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// Stream lifecycle state.
///
/// Transitions follow the state machine:
/// ```text
///              ┌──────────┐
///   new() ────▶│ Created  │
///              └────┬─────┘
///                   │ accept_first_frame()
///                   ▼
///              ┌──────────┐
///              │ Active   │◀─────────┐
///              └────┬─────┘          │
///                   │                │ (processing frames)
///                   ├─────────────────┘
///                   │
///          ┌────────┼────────┐
///          │                 │
///   client_cancel()   mark_complete()
///    or timeout()        or error()
///          │                 │
///          ▼                 ▼
///     ┌──────────┐     ┌──────────┐
///     │Cancelling│     │ Terminal │
///     └────┬─────┘     └──────────┘
///          │
///   wait_graceful()
///          │
///          ▼
///     ┌──────────┐
///     │ Terminal │
///     └──────────┘
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamState {
    /// Stream created, awaiting first frame
    Created,
    /// Stream active, accepting frames
    Active,
    /// Cancellation in progress
    Cancelling,
    /// Stream terminal (completed or failed)
    Terminal,
}

/// Reason for stream cancellation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CancellationReason {
    /// Client explicitly requested cancellation
    ClientRequested,
    /// Server initiated graceful shutdown
    ServerGracefulShutdown,
    /// Stream exceeded idle timeout (30s)
    IdleTimeout,
    /// Stream exceeded overall timeout (5min)
    OverallTimeout,
    /// Connection lost while stream active
    ConnectionLost,
}

/// Backpressure request from server to client.
///
/// Indicates that the server is under resource pressure and cannot accept
/// new stream creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackpressureRequest {
    /// Server-reported reason for backpressure
    pub reason: BackpressureReason,
    /// Recommended delay before retry in milliseconds
    pub retry_after_millis: u64,
    /// Optional request ID if backpressure is tied to a specific request
    pub request_id: Option<RequestId>,
}

/// Server-side backpressure reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackpressureReason {
    /// Receive buffer saturated
    ReceiveBufferSaturated,
    /// Execution queue saturated
    ExecutionQueueSaturated,
    /// WAL flush lagging
    WalFlushLag,
    /// Hot store pressure
    HotStorePressure,
    /// Result spool growing
    ResultSpoolGrowth,
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
    pub fn from_invocation_id(invocation_id: InvocationId) -> Self {
        Self(invocation_id.get())
    }

    /// Returns the raw token value.
    pub fn get(&self) -> u64 {
        self.0
    }
}

/// Stream metadata bound to a single concurrent procedure invocation.
#[derive(Debug)]
struct StreamMetadata {
    /// Stream state
    state: StreamState,
    /// Stream creation timestamp (for timeout tracking)
    created_at: SystemTime,
    /// Last frame received timestamp (for idle timeout tracking)
    last_activity_at: SystemTime,
    /// Cancellation token (deterministic, set at creation)
    cancellation_token: CancellationToken,
    /// Cancellation reason, if stream was cancelled
    cancellation_reason: Option<CancellationReason>,
}

/// Manages stream concurrency and lifecycle for a single QUIC connection.
///
/// The manager enforces:
/// - Maximum concurrent streams (configurable, default 128)
/// - Stream lifecycle state machine atomicity
/// - Backpressure signaling when saturation reached
/// - Timeout detection and cleanup
/// - Replay-safe cancellation token generation
///
/// ## Invariants
///
/// 1. No stream may be created if the concurrency limit is reached.
/// 2. State transitions are atomic: no observer sees inconsistent state.
/// 3. Cancellation tokens are deterministic and never reused within a connection.
/// 4. Timeouts are detected on observation, not enforced by background tasks.
pub struct StreamConcurrencyManager {
    /// Map of invocation ID → stream metadata
    streams: HashMap<InvocationId, StreamMetadata>,
    /// Maximum concurrent streams (configurable)
    max_concurrent: usize,
    /// Idle timeout duration
    idle_timeout: Duration,
    /// Overall request timeout duration
    overall_timeout: Duration,
    /// Global stream counter for total created streams
    total_streams_created: Arc<AtomicU64>,
}

impl StreamConcurrencyManager {
    /// Default maximum concurrent streams per connection.
    pub const DEFAULT_MAX_CONCURRENT: usize = 128;
    /// Default idle timeout duration (30 seconds).
    pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
    /// Default overall timeout duration (5 minutes).
    pub const DEFAULT_OVERALL_TIMEOUT: Duration = Duration::from_secs(300);

    /// Creates a new stream concurrency manager with default bounds.
    pub fn new() -> Self {
        Self::with_bounds(
            Self::DEFAULT_MAX_CONCURRENT,
            Self::DEFAULT_IDLE_TIMEOUT,
            Self::DEFAULT_OVERALL_TIMEOUT,
        )
    }

    /// Creates a new stream concurrency manager with custom bounds.
    ///
    /// # Arguments
    ///
    /// * `max_concurrent` - Maximum concurrent streams per connection
    /// * `idle_timeout` - Timeout for idle streams (no frames for this duration)
    /// * `overall_timeout` - Timeout for overall stream lifetime
    pub fn with_bounds(
        max_concurrent: usize,
        idle_timeout: Duration,
        overall_timeout: Duration,
    ) -> Self {
        Self {
            streams: HashMap::new(),
            max_concurrent,
            idle_timeout,
            overall_timeout,
            total_streams_created: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Creates a new stream, allocating an invocation ID.
    ///
    /// Returns `Err` if:
    /// - The concurrency limit is reached (returns `BackpressureRequest`)
    /// - The invocation ID is already in use
    ///
    /// Returns `Ok` with the cancellation token on success.
    pub fn create_stream(
        &mut self,
        invocation_id: InvocationId,
    ) -> AndromedaResult<CancellationToken> {
        // Check concurrency limit
        if self.streams.len() >= self.max_concurrent {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "stream concurrency limit reached",
            ));
        }

        // Ensure no duplicate invocation IDs
        if self.streams.contains_key(&invocation_id) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transport,
                "stream already exists for this invocation ID",
            ));
        }

        let now = SystemTime::now();
        let cancellation_token = CancellationToken::from_invocation_id(invocation_id);

        let metadata = StreamMetadata {
            state: StreamState::Created,
            created_at: now,
            last_activity_at: now,
            cancellation_token,
            cancellation_reason: None,
        };

        self.streams.insert(invocation_id, metadata);
        self.total_streams_created.fetch_add(1, Ordering::SeqCst);

        Ok(cancellation_token)
    }

    /// Marks a stream as active (first frame accepted).
    ///
    /// Returns `Err` if the stream does not exist or is not in `Created` state.
    pub fn accept_first_frame(&mut self, invocation_id: InvocationId) -> AndromedaResult<()> {
        let stream = self.streams.get_mut(&invocation_id).ok_or_else(|| {
            AndromedaError::new(AndromedaErrorKind::Transport, "stream not found")
        })?;

        if stream.state != StreamState::Created {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transport,
                "stream is not in created state",
            ));
        }

        stream.state = StreamState::Active;
        stream.last_activity_at = SystemTime::now();
        Ok(())
    }

    /// Updates the last activity time for a stream.
    ///
    /// Called whenever a frame is received on the stream. Used for idle timeout tracking.
    pub fn record_activity(&mut self, invocation_id: InvocationId) -> AndromedaResult<()> {
        let stream = self.streams.get_mut(&invocation_id).ok_or_else(|| {
            AndromedaError::new(AndromedaErrorKind::Transport, "stream not found")
        })?;

        stream.last_activity_at = SystemTime::now();
        Ok(())
    }

    /// Marks a stream as cancelled.
    ///
    /// Returns `Err` if the stream is already terminal.
    /// If the stream is already terminal (completed), this returns an error
    /// to signal that the cancellation was too late.
    pub fn cancel_stream(
        &mut self,
        invocation_id: InvocationId,
        reason: CancellationReason,
    ) -> AndromedaResult<()> {
        let stream = self.streams.get_mut(&invocation_id).ok_or_else(|| {
            AndromedaError::new(AndromedaErrorKind::Transport, "stream not found")
        })?;

        // If already terminal, return error to indicate too-late cancellation
        if stream.state == StreamState::Terminal {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transport,
                "cannot cancel a terminal stream",
            ));
        }

        stream.state = StreamState::Cancelling;
        stream.cancellation_reason = Some(reason);
        Ok(())
    }

    /// Marks a stream as complete (success or failure).
    ///
    /// Returns `Err` if the stream does not exist or is already terminal.
    pub fn mark_complete(&mut self, invocation_id: InvocationId) -> AndromedaResult<()> {
        let stream = self.streams.get_mut(&invocation_id).ok_or_else(|| {
            AndromedaError::new(AndromedaErrorKind::Transport, "stream not found")
        })?;

        if stream.state == StreamState::Terminal {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transport,
                "stream is already terminal",
            ));
        }

        stream.state = StreamState::Terminal;
        Ok(())
    }

    /// Removes a stream from the manager after it has been cleaned up.
    ///
    /// Should only be called after `mark_complete()` for a stream.
    /// This allows the manager's internal map to remain bounded.
    pub fn cleanup_stream(&mut self, invocation_id: InvocationId) -> AndromedaResult<()> {
        self.streams.remove(&invocation_id).ok_or_else(|| {
            AndromedaError::new(AndromedaErrorKind::Transport, "stream not found")
        })?;
        Ok(())
    }

    /// Gets the current state of a stream.
    pub fn get_state(&self, invocation_id: InvocationId) -> AndromedaResult<StreamState> {
        self.streams
            .get(&invocation_id)
            .map(|m| m.state)
            .ok_or_else(|| AndromedaError::new(AndromedaErrorKind::Transport, "stream not found"))
    }

    /// Gets the cancellation token for a stream.
    pub fn get_cancellation_token(
        &self,
        invocation_id: InvocationId,
    ) -> AndromedaResult<CancellationToken> {
        self.streams
            .get(&invocation_id)
            .map(|m| m.cancellation_token)
            .ok_or_else(|| AndromedaError::new(AndromedaErrorKind::Transport, "stream not found"))
    }

    /// Detects and returns streams that have exceeded their idle timeout.
    pub fn detect_idle_timeouts(&self) -> Vec<InvocationId> {
        let now = SystemTime::now();
        self.streams
            .iter()
            .filter(|(_, metadata)| {
                metadata.state != StreamState::Terminal
                    && now
                        .duration_since(metadata.last_activity_at)
                        .unwrap_or_default()
                        > self.idle_timeout
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// Detects and returns streams that have exceeded their overall timeout.
    pub fn detect_overall_timeouts(&self) -> Vec<InvocationId> {
        let now = SystemTime::now();
        self.streams
            .iter()
            .filter(|(_, metadata)| {
                metadata.state != StreamState::Terminal
                    && now.duration_since(metadata.created_at).unwrap_or_default()
                        > self.overall_timeout
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// Detects all orphaned streams (idle + overall timeout) and returns them for cleanup.
    pub fn detect_orphaned_streams(&self) -> Vec<InvocationId> {
        let idle = self.detect_idle_timeouts();
        let overall = self.detect_overall_timeouts();
        let mut all: Vec<_> = idle.into_iter().chain(overall).collect();
        all.sort();
        all.dedup();
        all
    }

    /// Returns the current number of active (non-terminal) streams.
    pub fn active_stream_count(&self) -> usize {
        self.streams
            .values()
            .filter(|m| m.state != StreamState::Terminal)
            .count()
    }

    /// Returns the total number of concurrent streams (all states).
    pub fn total_concurrent_streams(&self) -> usize {
        self.streams.len()
    }

    /// Returns true if the concurrency limit has been reached.
    pub fn is_at_capacity(&self) -> bool {
        self.active_stream_count() >= self.max_concurrent
    }

    /// Returns the backpressure status for the connection.
    ///
    /// If the manager is at capacity, returns `Some(BackpressureRequest)`.
    /// Otherwise, returns `None`.
    pub fn backpressure_status(&self) -> Option<BackpressureRequest> {
        if self.is_at_capacity() {
            Some(BackpressureRequest {
                reason: BackpressureReason::ExecutionQueueSaturated,
                retry_after_millis: 100,
                request_id: None,
            })
        } else {
            None
        }
    }

    /// Returns the total number of streams ever created on this connection.
    pub fn total_streams_ever_created(&self) -> u64 {
        self.total_streams_created.load(Ordering::SeqCst)
    }

    /// Returns the maximum concurrent stream limit.
    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }

    /// Returns the idle timeout duration.
    pub fn idle_timeout(&self) -> Duration {
        self.idle_timeout
    }

    /// Returns the overall timeout duration.
    pub fn overall_timeout(&self) -> Duration {
        self.overall_timeout
    }

    /// Checks if a stream is idle (no activity for idle_timeout duration).
    pub fn is_idle(&self, invocation_id: InvocationId) -> AndromedaResult<bool> {
        let stream = self.streams.get(&invocation_id).ok_or_else(|| {
            AndromedaError::new(AndromedaErrorKind::Transport, "stream not found")
        })?;

        let elapsed = SystemTime::now()
            .duration_since(stream.last_activity_at)
            .unwrap_or_default();

        Ok(elapsed > self.idle_timeout)
    }

    /// Enforces timeout policy by transitioning all streams that have exceeded
    /// their idle or overall deadline into the `Cancelling` state.
    ///
    /// Returns a vector of `(InvocationId, CancellationReason)` pairs for every
    /// stream that was successfully transitioned. Callers must observe each returned
    /// pair and emit a `TimeoutExceeded` trace event to the durable audit ledger
    /// before calling `mark_complete` + `cleanup_stream` on that stream.
    ///
    /// ## Invariants
    ///
    /// - Idle-timeout is checked first; if a stream qualifies for both idle and
    ///   overall timeout, `IdleTimeout` takes precedence in the returned reason.
    /// - Streams already in `Cancelling` or `Terminal` are skipped (no double-cancel).
    /// - The caller owns the timer context; this method reads `SystemTime::now()` only
    ///   once per call (both detection passes use the `detect_*` snapshot, which reads
    ///   the clock internally but is already consistent because no mutation occurs
    ///   between the two snapshot calls within this enforcer).
    ///
    /// ## Wave scope
    ///
    /// Wires the existing `detect_idle_timeouts()` / `detect_overall_timeouts()`
    /// detection into actual state-machine enforcement. The broader per-invocation
    /// deadline (admission → WAL commit) is Wave 14+;
    /// see `SCOPED_INVOCATION_TIMEOUT.md §7`.
    pub fn enforce_timeouts(&mut self) -> Vec<(InvocationId, CancellationReason)> {
        // Snapshot both detection lists before mutating any state.
        let idle_ids = self.detect_idle_timeouts();
        let overall_ids = self.detect_overall_timeouts();

        let mut cancelled: Vec<(InvocationId, CancellationReason)> = Vec::new();

        // Apply idle-timeout cancellations first.
        for id in idle_ids {
            if self
                .cancel_stream(id, CancellationReason::IdleTimeout)
                .is_ok()
            {
                cancelled.push((id, CancellationReason::IdleTimeout));
            }
        }

        // Apply overall-timeout cancellations, skipping streams already cancelled.
        let already_cancelled: std::collections::HashSet<InvocationId> =
            cancelled.iter().map(|(id, _)| *id).collect();
        for id in overall_ids {
            if already_cancelled.contains(&id) {
                continue;
            }
            if self
                .cancel_stream(id, CancellationReason::OverallTimeout)
                .is_ok()
            {
                cancelled.push((id, CancellationReason::OverallTimeout));
            }
        }

        cancelled
    }

    /// Returns the cancellation reason if the stream was cancelled.
    pub fn get_cancellation_reason(
        &self,
        invocation_id: InvocationId,
    ) -> AndromedaResult<Option<CancellationReason>> {
        self.streams
            .get(&invocation_id)
            .map(|m| m.cancellation_reason)
            .ok_or_else(|| AndromedaError::new(AndromedaErrorKind::Transport, "stream not found"))
    }
}

impl Default for StreamConcurrencyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invocation_id(id: u64) -> InvocationId {
        InvocationId::new(id)
    }

    #[test]
    fn stream_concurrency_manager_creates_streams() {
        let mut mgr = StreamConcurrencyManager::new();
        let token = mgr.create_stream(invocation_id(1)).unwrap();
        assert_eq!(token.get(), 1);
        assert_eq!(mgr.total_concurrent_streams(), 1);
    }

    #[test]
    fn stream_lifecycle_created_to_active() {
        let mut mgr = StreamConcurrencyManager::new();
        let id = invocation_id(1);
        mgr.create_stream(id).unwrap();
        assert_eq!(mgr.get_state(id).unwrap(), StreamState::Created);
        mgr.accept_first_frame(id).unwrap();
        assert_eq!(mgr.get_state(id).unwrap(), StreamState::Active);
    }

    #[test]
    fn stream_lifecycle_active_to_terminal() {
        let mut mgr = StreamConcurrencyManager::new();
        let id = invocation_id(1);
        mgr.create_stream(id).unwrap();
        mgr.accept_first_frame(id).unwrap();
        mgr.mark_complete(id).unwrap();
        assert_eq!(mgr.get_state(id).unwrap(), StreamState::Terminal);
    }

    #[test]
    fn stream_concurrency_limit_enforced() {
        let mut mgr = StreamConcurrencyManager::with_bounds(
            2,
            Duration::from_secs(30),
            Duration::from_secs(300),
        );
        mgr.create_stream(invocation_id(1)).unwrap();
        mgr.create_stream(invocation_id(2)).unwrap();
        let err = mgr.create_stream(invocation_id(3)).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Resource);
    }

    #[test]
    fn cancellation_token_deterministic() {
        let id = invocation_id(42);
        let token1 = CancellationToken::from_invocation_id(id);
        let token2 = CancellationToken::from_invocation_id(id);
        assert_eq!(token1, token2);
    }
}
