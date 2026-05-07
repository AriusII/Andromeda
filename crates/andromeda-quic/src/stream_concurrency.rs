//! Stream concurrency, cancellation, timeout, and backpressure state for QUIC
//! streams carrying Andromeda procedure invocations.

mod admission;
mod backpressure;
mod errors;
mod limits;
mod state;

pub use backpressure::BackpressureRequest;
pub use limits::StreamConcurrencyLimits;
pub use state::{CancellationReason, CancellationToken, StreamState};

use andromeda_core::{AndromedaResult, InvocationId};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use self::state::StreamMetadata;

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
    streams: HashMap<InvocationId, StreamMetadata>,
    limits: StreamConcurrencyLimits,
    total_streams_created: Arc<AtomicU64>,
}

impl StreamConcurrencyManager {
    /// Default maximum concurrent streams per connection.
    pub const DEFAULT_MAX_CONCURRENT: usize = StreamConcurrencyLimits::DEFAULT_MAX_CONCURRENT;
    /// Default idle timeout duration (30 seconds).
    pub const DEFAULT_IDLE_TIMEOUT: Duration = StreamConcurrencyLimits::DEFAULT_IDLE_TIMEOUT;
    /// Default overall timeout duration (5 minutes).
    pub const DEFAULT_OVERALL_TIMEOUT: Duration = StreamConcurrencyLimits::DEFAULT_OVERALL_TIMEOUT;

    /// Creates a new stream concurrency manager with default bounds.
    pub fn new() -> Self {
        Self::with_limits(StreamConcurrencyLimits::default())
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
        Self::with_limits(StreamConcurrencyLimits::new(
            max_concurrent,
            idle_timeout,
            overall_timeout,
        ))
    }

    /// Creates a new stream concurrency manager from a bounded limit contract.
    pub fn with_limits(limits: StreamConcurrencyLimits) -> Self {
        Self {
            streams: HashMap::new(),
            limits,
            total_streams_created: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Creates a new stream and returns its cancellation token.
    pub fn create_stream(
        &mut self,
        invocation_id: InvocationId,
    ) -> AndromedaResult<CancellationToken> {
        let token = admission::admit_stream(
            &mut self.streams,
            self.limits,
            invocation_id,
            SystemTime::now(),
        )?;
        self.total_streams_created.fetch_add(1, Ordering::SeqCst);
        Ok(token)
    }

    /// Marks a stream as active (first frame accepted).
    ///
    /// Returns `Err` if the stream does not exist or is not in `Created` state.
    pub fn accept_first_frame(&mut self, invocation_id: InvocationId) -> AndromedaResult<()> {
        let stream = self.stream_mut(invocation_id)?;

        if !stream.is_created() {
            return Err(errors::invalid_stream_state(
                "stream is not in created state",
            ));
        }

        stream.mark_active(SystemTime::now());
        Ok(())
    }

    /// Updates the last activity time for a stream.
    ///
    /// Called whenever a frame is received on the stream. Used for idle timeout tracking.
    pub fn record_activity(&mut self, invocation_id: InvocationId) -> AndromedaResult<()> {
        self.stream_mut(invocation_id)?
            .record_activity(SystemTime::now());
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
        let stream = self.stream_mut(invocation_id)?;

        if stream.is_terminal() {
            return Err(errors::invalid_stream_state(
                "cannot cancel a terminal stream",
            ));
        }

        stream.mark_cancelling(reason);
        Ok(())
    }

    /// Marks a stream as complete (success or failure).
    ///
    /// Returns `Err` if the stream does not exist or is already terminal.
    pub fn mark_complete(&mut self, invocation_id: InvocationId) -> AndromedaResult<()> {
        let stream = self.stream_mut(invocation_id)?;

        if stream.is_terminal() {
            return Err(errors::invalid_stream_state("stream is already terminal"));
        }

        stream.mark_terminal();
        Ok(())
    }

    /// Removes a stream from the manager after it has been cleaned up.
    ///
    /// Should only be called after `mark_complete()` for a stream.
    /// This allows the manager's internal map to remain bounded.
    pub fn cleanup_stream(&mut self, invocation_id: InvocationId) -> AndromedaResult<()> {
        self.streams
            .remove(&invocation_id)
            .ok_or_else(errors::stream_not_found)?;
        Ok(())
    }

    /// Gets the current state of a stream.
    pub fn get_state(&self, invocation_id: InvocationId) -> AndromedaResult<StreamState> {
        self.stream(invocation_id).map(StreamMetadata::state)
    }

    /// Gets the cancellation token for a stream.
    pub fn get_cancellation_token(
        &self,
        invocation_id: InvocationId,
    ) -> AndromedaResult<CancellationToken> {
        self.stream(invocation_id)
            .map(StreamMetadata::cancellation_token)
    }

    /// Detects and returns streams that have exceeded their idle timeout.
    pub fn detect_idle_timeouts(&self) -> Vec<InvocationId> {
        self.detect_idle_timeouts_at(SystemTime::now())
    }

    /// Detects and returns streams that have exceeded their overall timeout.
    pub fn detect_overall_timeouts(&self) -> Vec<InvocationId> {
        self.detect_overall_timeouts_at(SystemTime::now())
    }

    /// Detects all orphaned streams and returns them for cleanup.
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
            .filter(|metadata| !metadata.is_terminal())
            .count()
    }

    /// Returns the total number of concurrent streams (all states).
    pub fn total_concurrent_streams(&self) -> usize {
        self.streams.len()
    }

    /// Returns true if the concurrency limit has been reached.
    pub fn is_at_capacity(&self) -> bool {
        self.active_stream_count() >= self.limits.max_concurrent()
    }

    /// Returns the backpressure status for the connection.
    pub fn backpressure_status(&self) -> Option<BackpressureRequest> {
        self.is_at_capacity()
            .then(backpressure::capacity_backpressure)
    }

    /// Returns the total number of streams ever created on this connection.
    pub fn total_streams_ever_created(&self) -> u64 {
        self.total_streams_created.load(Ordering::SeqCst)
    }

    /// Returns the maximum concurrent stream limit.
    pub fn max_concurrent(&self) -> usize {
        self.limits.max_concurrent()
    }

    /// Returns the idle timeout duration.
    pub fn idle_timeout(&self) -> Duration {
        self.limits.idle_timeout()
    }

    /// Returns the overall timeout duration.
    pub fn overall_timeout(&self) -> Duration {
        self.limits.overall_timeout()
    }

    /// Checks if a stream is idle (no activity for idle_timeout duration).
    pub fn is_idle(&self, invocation_id: InvocationId) -> AndromedaResult<bool> {
        self.stream(invocation_id)
            .map(|stream| stream.activity_is_idle_at(SystemTime::now(), self.limits.idle_timeout()))
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
    ///   once per call.
    ///
    /// This enforces only stream-local idle and overall deadlines. Broader
    /// invocation deadlines are owned by admission/runtime layers.
    pub fn enforce_timeouts(&mut self) -> Vec<(InvocationId, CancellationReason)> {
        let now = SystemTime::now();
        let idle_ids = self.detect_idle_timeouts_at(now);
        let overall_ids = self.detect_overall_timeouts_at(now);

        let mut cancelled: Vec<(InvocationId, CancellationReason)> = Vec::new();

        for id in idle_ids {
            if self.cancel_timeout_stream(id, CancellationReason::IdleTimeout) {
                cancelled.push((id, CancellationReason::IdleTimeout));
            }
        }

        let already_cancelled: HashSet<InvocationId> =
            cancelled.iter().map(|(id, _)| *id).collect();
        for id in overall_ids {
            if already_cancelled.contains(&id) {
                continue;
            }
            if self.cancel_timeout_stream(id, CancellationReason::OverallTimeout) {
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
        self.stream(invocation_id)
            .map(StreamMetadata::cancellation_reason)
    }

    fn detect_idle_timeouts_at(&self, now: SystemTime) -> Vec<InvocationId> {
        self.streams
            .iter()
            .filter(|(_, metadata)| metadata.is_idle_at(now, self.limits.idle_timeout()))
            .map(|(id, _)| *id)
            .collect()
    }

    fn detect_overall_timeouts_at(&self, now: SystemTime) -> Vec<InvocationId> {
        self.streams
            .iter()
            .filter(|(_, metadata)| {
                metadata.is_overall_timeout_at(now, self.limits.overall_timeout())
            })
            .map(|(id, _)| *id)
            .collect()
    }

    fn cancel_timeout_stream(
        &mut self,
        invocation_id: InvocationId,
        reason: CancellationReason,
    ) -> bool {
        let Ok(stream) = self.stream_mut(invocation_id) else {
            return false;
        };
        if matches!(
            stream.state(),
            StreamState::Cancelling | StreamState::Terminal
        ) {
            return false;
        }

        stream.mark_cancelling(reason);
        true
    }

    fn stream(&self, invocation_id: InvocationId) -> AndromedaResult<&StreamMetadata> {
        self.streams
            .get(&invocation_id)
            .ok_or_else(errors::stream_not_found)
    }

    fn stream_mut(&mut self, invocation_id: InvocationId) -> AndromedaResult<&mut StreamMetadata> {
        self.streams
            .get_mut(&invocation_id)
            .ok_or_else(errors::stream_not_found)
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
    use andromeda_core::AndromedaErrorKind;

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
        let token1 = StreamConcurrencyManager::new().create_stream(id).unwrap();
        let token2 = StreamConcurrencyManager::new().create_stream(id).unwrap();
        assert_eq!(token1, token2);
    }
}
