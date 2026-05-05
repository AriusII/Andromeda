# DEC-020: Stream Concurrency, Cancellation, and Backpressure

## Status

**Designed**. Module implementation (stream_concurrency.rs) complete; integration with D4 executor dispatch and F1 WAL shipping deferred to delivery phases.

## Context

Andromeda V0 foundation layers have completed:
- **D1–D3**: QUIC frame layer, session lifecycle, mTLS identity extraction.
- **D4**: Executor bridge to QUIC streams (awaiting this concurrency design).
- **F1**: WAL shipping runtime (will integrate with stream backpressure).

The next layer is **D5: Stream Concurrency and Cancellation**. This decision specifies the stream lifecycle state machine, backpressure protocol, cancellation semantics, timeout policies, and race condition safety that will guide stream management within each QUIC connection.

## Problem Statement

The QUIC transport must coordinate concurrent procedure invocations on multiple bidirectional command streams. The design must:

1. **Prevent Resource Exhaustion** — Enforce a maximum concurrent stream limit (V0: 128 per connection).
2. **Signal Saturation Gracefully** — Use backpressure signaling when capacity is reached.
3. **Support Cancellation from Three Paths** — Client explicit cancel, server graceful shutdown, timeout-detected orphans.
4. **Ensure Atomicity** — Simultaneous cancel + completion must resolve deterministically with no race.
5. **Enable Timeout Detection** — Idle streams (30s) and overall timeout (5min) must be detectable without background tasks.
6. **Provide Deterministic Tokens** — Cancellation tokens must be replay-safe and tied to invocation IDs.
7. **Integrate with Executor Dispatch** — D4 will use stream ID → InvocationId mapping with tokens for correlation.

## Decision

### 1. Stream Lifecycle State Machine

A stream follows a fixed state machine with four states:

```
              ┌──────────┐
   new() ────▶│ Created  │
              └────┬─────┘
                   │ accept_first_frame()
                   ▼
              ┌──────────┐
              │ Active   │◀─────────┐
              └────┬─────┘          │
                   │                │ (processing frames)
                   ├─────────────────┘
                   │
          ┌────────┼────────┐
          │                 │
   client_cancel()   mark_complete()
    or timeout()        or error()
          │                 │
          ▼                 ▼
     ┌──────────┐     ┌──────────┐
     │Cancelling│     │ Terminal │
     └────┬─────┘     └──────────┘
          │
   wait_graceful()
          │
          ▼
     ┌──────────┐
     │ Terminal │
     └──────────┘
```

**Invariants:**

- Only the `StreamConcurrencyManager` may change state.
- Transitions are atomic: no observer sees intermediate state.
- Terminal state is irreversible.
- Once Terminal, all operations on the stream (cancel, complete) return errors.

### 2. Backpressure Protocol

**Backpressure Request** is sent by the server when resource saturation is detected:

```rust
pub struct BackpressureRequest {
    pub reason: BackpressureReason,          // Why we're backpressured
    pub retry_after_millis: u64,             // Recommended retry delay
    pub request_id: Option<RequestId>,       // If request-scoped
}

pub enum BackpressureReason {
    ReceiveBufferSaturated,      // QUIC recv buffer full
    ExecutionQueueSaturated,     // Executor queue at limit
    WalFlushLag,                 // WAL not flushing fast enough
    HotStorePressure,            // Hot store quota exceeded
    ResultSpoolGrowth,           // Result stream backlog growing
}
```

**Sending Backpressure:**

- Server detects concurrency limit reached (all 128 streams active).
- Emits `BackpressureRequest` on the diagnostic stream (reliable, carries request_id).
- Also may emit on telemetry datagram (fire-and-forget, limited payload).

**Receiving Backpressure:**

- Client receives `BackpressureRequest`.
- Ceases new stream creation for the recommended delay.
- After delay, retries stream creation.

**Recovery on Stream Completion:**

- When a stream completes and is removed from the manager, active stream count decreases.
- If previously at capacity, backpressure is lifted.
- Client observes no backpressure on next stream create attempt.

### 3. Cancellation Semantics

Cancellation can originate from three sources, each with distinct guarantees:

#### 3.1 Client-Initiated Cancellation

**Trigger:** Client sends explicit cancellation frame.

**Path:**
1. Client invokes `cancel_stream(invocation_id, CancellationReason::ClientRequested)`.
2. Stream moves to `Cancelling` state.
3. Server receives cancellation frame on the command stream.
4. Server stops processing frames and begins graceful shutdown of result stream.
5. Result stream transitions to Terminal with completion frame.
6. Stream manager calls `mark_complete()` and `cleanup_stream()`.

**Guarantee:** At-least-once signal delivery (cancellation frame is retried if lost on command stream).

#### 3.2 Server Graceful Shutdown

**Trigger:** Server is draining (e.g., controlled shutdown, version upgrade).

**Path:**
1. Server emits `begin_drain()` on connection.
2. For each active stream, server invokes `cancel_stream(id, CancellationReason::ServerGracefulShutdown)`.
3. Streams move to `Cancelling` state.
4. Result streams flush any pending results, then emit completion frame.
5. Client receives completion frame and is responsible for cleanup.

**Guarantee:** Best-effort (no explicit cancellation frame sent; client relies on completion frame).

#### 3.3 Orphan Cleanup (Timeout)

**Trigger:** Stream has not received any frames for idle timeout (30s) or overall lifetime exceeds 5 minutes.

**Detection:**
1. Manager's `detect_idle_timeouts()` or `detect_overall_timeouts()` called by executor or connection handler.
2. Returns list of timed-out stream IDs.
3. For each, executor calls `cancel_stream(id, CancellationReason::IdleTimeout)` or `ConnectionLost`.

**Guarantee:** Cleanup is best-effort; stream resources will be reclaimed when detected.

### 4. Concurrency Bounds

**V0 Configuration:**

```rust
pub const DEFAULT_MAX_CONCURRENT: usize = 128;
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_OVERALL_TIMEOUT: Duration = Duration::from_secs(300);  // 5 minutes
```

**Tuning:**

Configuration is per-connection, set at listener level during `SurfaceListenerConfig` instantiation (deferred to D5-IMPL phase). Future:

```rust
ListenerConfig {
    surface_plane: SurfacePlane::Application,
    stream_concurrency_max: 256,           // Tunable per plane
    idle_timeout: Duration::from_secs(45),
    overall_timeout: Duration::from_secs(600),
}
```

**Enforcement:**

- When `create_stream()` is called and active stream count >= max_concurrent, return error + backpressure signal.
- No background eviction task; cleanup is driven by explicit timeout detection.

### 5. Cancellation Token Generation

Cancellation tokens are generated deterministically from the invocation ID:

```rust
pub fn from_invocation_id(invocation_id: InvocationId) -> CancellationToken {
    CancellationToken(invocation_id.get())
}
```

**Properties:**

- **Deterministic**: Same invocation ID produces same token every time (replay-safe).
- **Unique per Connection**: InvocationId is globally unique within a QUIC connection.
- **Immutable**: Once assigned, token cannot change.

**Usage in D4 Executor Dispatch:**

1. QUIC stream created; manager allocates InvocationId and CancellationToken.
2. Token is passed to executor along with invocation context.
3. If client sends cancellation frame mid-execution, executor receives cancellation event with token.
4. Executor can verify token matches the active invocation (prevents replay/confusion attacks).

### 6. Race Condition Safety

**Key Race: Simultaneous Cancel + Completion**

```
Timeline:
  T0: Client sends cancellation frame
  T1: Executor finishes computation, sends completion frame
  T2: Manager receives both signals concurrently
```

**Resolution:**

1. Manager maintains a single `state` field per stream (atomic in Rust due to single-threaded design).
2. First state transition (cancel OR complete) wins; second is queued or rejected.
3. If cancel wins: state → Cancelling. Subsequent `mark_complete()` succeeds (Cancelling → Terminal).
4. If complete wins: state → Terminal. Subsequent `cancel_stream()` returns error (too late).
5. The cancellation reason is preserved even if terminal via completion.

**Proof by Atomic State:**

In the Rust implementation, the manager uses a `HashMap<InvocationId, StreamMetadata>` where `StreamMetadata.state` is a single `StreamState` enum. Rust's ownership model ensures:
- Only one mutable reference exists at a time.
- State transitions are fully atomic (no partial updates).
- No two threads can execute different transitions simultaneously.

### 7. Integration Points

#### 7.1 D4 Executor Dispatch Bridge

**Stream → InvocationContext Mapping:**

```rust
// In ExecutorDispatchBridge::admit_invocation()
let invocation_id = stream_id_to_invocation_id(quic_stream_id);
let token = mgr.create_stream(invocation_id)?;  // Allocate stream

let invocation = InvocationContext {
    invocation_id,
    cancellation_token: token,
    req_body: frame.payload,
    // ...
};

executor.admit(invocation)?;
```

**Cancellation Flow:**

```rust
// Client sends cancellation frame on command stream
conn.receive_frame(&cancellation_frame)?;

// D4 dispatches to stream manager
mgr.cancel_stream(invocation_id, CancellationReason::ClientRequested)?;

// Executor sees cancellation event
executor.on_cancellation(invocation_id)?;
```

#### 7.2 F1 WAL Shipping Integration

**Backpressure Flow:**

```rust
// Executor is backpressured (execution queue full)
executor_backpressure_detected();

// Manager detects it and issues backpressure signal
let request = BackpressureRequest {
    reason: ExecutionQueueSaturated,
    retry_after_millis: 100,
    request_id: None,
};

// F1 shipping thread observes this and may throttle WAL replica shipping
// to avoid cascading overload
```

**LSN Correlation:**

- WAL shipping is independent of stream concurrency.
- However, if all streams are backpressured, F1 may defer replica shipping to reduce memory pressure.
- No direct coupling; integration deferred to F1-IMPL phase.

### 8. No Unsafe Code

This module is declared `#![forbid(unsafe_code)]` in the parent crate. All stream management uses:
- Standard Rust collections (`HashMap`, `Vec`).
- Atomic operations only for global counters (via `Arc<AtomicU64>`).
- No raw pointers, manual memory management, or unsafe blocks.

### 9. Testing Strategy

20 test scenarios in `stream_concurrency_contract.rs`:

1. **test_stream_created_state_initializes_correctly** — Initial state is Created.
2. **test_backpressure_blocks_new_streams_when_limit_reached** — Concurrency limit enforced.
3. **test_client_cancellation_triggers_graceful_shutdown** — Client cancel path.
4. **test_concurrent_stream_operations_are_atomic** — No interference between streams.
5. **test_stream_timeout_cleanup** — Idle timeout detection works.
6. **test_orphan_stream_detection** — Orphan detection catches timed-out streams.
7. **test_race_simultaneous_cancel_and_completion** — Race resolution is correct.
8. **test_backpressure_recovery_on_stream_completion** — Backpressure lifts when capacity freed.
9. **test_cancellation_token_deterministic_and_replay_safe** — Token generation is deterministic.
10. **test_invalid_state_transitions_rejected** — State machine enforces invariants.
11. **test_duplicate_invocation_ids_rejected** — No duplicate streams.
12. **test_server_graceful_shutdown_cancellation** — Server shutdown path.
13. **test_overall_timeout_detection** — Overall timeout detection.
14. **test_stream_activity_recording** — Activity tracking for idle detection.
15. **test_total_streams_ever_created_counter** — Lifelong counter persists.
16. **test_boundary_conditions_on_timeouts** — Timeout boundary conditions.
17. **test_orphan_cleanup_workflow** — Cleanup workflow end-to-end.
18. **test_configuration_accessors** — Configuration is queryable.
19. **test_connection_lost_cancellation** — Connection loss cancellation.
20. **test_backpressure_request_properties** — Backpressure signal properties.

All tests pass with no unsafe code and no blocking I/O in state transitions.

### 10. Open Integration Questions (Deferred)

1. **D4 Executor Dispatch**: How does executor correlation happen? Via request_id or invocation_id? (Answer TBD in D4-IMPL).
2. **F1 WAL Shipping**: Should F1 shipping thread honor backpressure signals on the telemetry datagram? (Answer TBD in F1-IMPL).
3. **Connection Drain Sequence**: When draining, does server wait for all streams to Terminal before close? (Answer TBD in D5-IMPL phase).
4. **Timeout Polling**: Who calls `detect_orphaned_streams()`? Executor? Connection? (Answer TBD in D5-IMPL phase).

## Rationale

### Why Fixed State Machine Over Event Loop?

A fixed state machine (Created → Active → Cancelling/Terminal) is simpler, more testable, and easier to prove correct than an event loop. The state is immutable from outside, reducing coupling.

### Why Deterministic Tokens Over Random UUIDs?

Deterministic tokens tie cancellation identity to invocation identity. This is replay-safe and allows correlation without an external registry. Random UUIDs would require a lookup table on the receiver side.

### Why Per-Connection Limit Over Global Limit?

Each QUIC connection is independent. A global limit would couple connections and complicate resource accounting. Per-connection limits are simpler and align with how QUIC itself is connection-scoped.

### Why No Background Timeout Task?

Background timeout tasks introduce async complexity and resource overhead. Timeout detection on observation (when callers check streams) is simpler and fits Andromeda's synchronous design. The executor/connection layer is responsible for periodic polling.

### Why 128 Streams Default?

128 concurrent procedure invocations per connection is a conservative default:
- Typical RPC deployments rarely exceed 100 concurrent calls per connection.
- 128 provides headroom without excessive memory overhead.
- Tunable per plane for future flexibility.

## Acceptance Criteria

- ✅ `StreamConcurrencyManager` module compiles without unsafe code.
- ✅ All 20 test scenarios pass.
- ✅ No race conditions between cancel and completion (proven by state machine).
- ✅ Backpressure model prevents stream exhaustion (concurrency limit enforced).
- ✅ Timeout cleanup maintains bounded resource usage (orphan detection works).
- ✅ Cancellation tokens are deterministic and replay-safe.
- ✅ Decision record matches implementation exactly.

## Related Decisions

- **DEC-018**: mTLS identity extraction on QUIC (provides certificate scope for D4 dispatch).
- **DEC-019**: F1 WAL shipping runtime (will integrate with backpressure signals).
- **DEC-014**: Rust crate module structure (stream_concurrency.rs is a new module in andromeda-quic).

## Future Work

1. **D4-IMPL**: Integrate with executor dispatch bridge to bind streams to invocations.
2. **D5-IMPL Phase**: Implement executor/connection layer timeout polling.
3. **D5-IMPL Phase**: Wire backpressure signals to F1 shipping and executor admission.
4. **Performance Tuning**: Benchmark stream allocation/deallocation; consider object pooling if needed.
5. **Observability**: Add metrics for stream creation, cancellation, timeout rates.
