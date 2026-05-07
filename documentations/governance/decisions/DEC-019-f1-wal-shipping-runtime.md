# DEC-019: F1 — WAL Shipping Runtime Model

## Status

**Designed**. Specification complete; implementation deferred to F1-IMPL. Test scaffolding and integration points defined.

## Context

Andromeda V0 HA/DR foundation has completed:
- **B6:** Crash/restart visibility tests validate that WAL is durable before transaction visibility.
- **D3:** mTLS identity extraction ensures secure primary-replica authentication on QUIC connections.
- **D2:** HA/DR surface plane listener scaffold establishes per-plane listener contracts.
- **D1/DEC-017:** QUIC runtime (quinn + rustls) selected; non-default feature deferred.

The next layer is the **WAL shipping runtime**: the data flow that moves committed WAL segments from primary to replicas. This decision specifies the protocol model, LSN correlation invariants, replica backpressure mechanism, and fencing decision points that will guide F1 implementation and F3 quorum consensus.

## Problem Statement

The shipping runtime must coordinate:
1. **Primary WriterThread** flushes durable records to disk.
2. **Primary ShippingThread** polls durable_lsn, reads committed segments, and sends via QUIC.
3. **Replica ReceivingThread** reads QUIC stream, validates checksum, appends to local WAL.
4. **Fencing decision point** at primary: should writes be blocked if a replica is lost?

The design must:
- Ensure durability before visibility (no shipped-but-not-durable segments).
- Enforce cryptographic integrity (checksum validation at replica).
- Maintain LSN chain contiguity (gap detection triggers fencing).
- Make fencing decisions explicit (async vs. quorum mode based on replica count).
- Produce auditable trace events for all errors.
- Avoid silent failures.

## Decision

### 1. Segment Shipping Protocol

A **segment** is an immutable group of consecutive WAL records. Segments are the atomic unit of replication.

#### Segment Descriptor

```rust
pub struct ShippingSegmentDescriptor {
    pub segment_id: u64,           // Monotonic identifier
    pub start_lsn: Lsn,           // First LSN (inclusive)
    pub end_lsn: Lsn,             // Last LSN (inclusive)
    pub record_count: usize,      // Number of records
    pub checksum: u64,            // FNV-1a 64-bit over all record bytes
}
```

**Invariants:**
- `segment_id` is unique and monotonically increasing.
- `start_lsn < end_lsn` (non-empty segment).
- `record_count` matches the number of records in the segment.
- `checksum` is computed deterministically from record bytes.

#### Shipping Condition

A segment is **shippable** if:
```
segment.end_lsn <= primary.durable_lsn
```

The primary's WriterThread flushes records to disk, updating `durable_lsn`. The ShippingThread polls `durable_lsn` and ships any segment whose `end_lsn` is now durable.

**Doctrine:** Durability-before-visibility. No record is shipped until it is durable at the primary.

#### Shipping Envelope

```rust
pub struct ShippingSegmentEnvelope<'a> {
    pub descriptor: ShippingSegmentDescriptor,
    pub records: &'a [WalRecord],
}
```

The envelope carries the segment metadata plus borrowed record bytes. It is validated on both sender and receiver:

**Sender (primary):**
1. Validate descriptor consistency (LSN range, record count).
2. Validate checksum matches computed value.
3. Emit as telemetry.

**Receiver (replica):**
1. Validate descriptor consistency (LSN range, record count).
2. Validate checksum matches received records.
3. Delegate to `WalShipmentBatch::validate()` for LSN chain validation.
4. Append validated records to local WAL.
5. Update `wal_received_lsn`.

**Error handling:**
- Checksum mismatch → `FencingEvent::ReplicaChecksumMismatch` → connection fencing.
- Record count mismatch → structure error → connection fencing.
- Checksum mismatch after replica re-receives the segment → repeat fencing.

### 2. LSN Correlation Model

The primary and replicas must maintain explicit LSN positions to coordinate replication mode and detect lag.

#### Primary Side

```rust
pub struct LsnCorrelationState {
    pub shipped_lsn_by_replica: HashMap<u64, Lsn>,
    pub replica_received_lsn: Lsn,
}
```

**`shipped_lsn_by_replica[replica_id]`** = highest LSN sent to that replica (acknowledged by replica).

This tracks what the replica _told us_ it received. The replica may have received more (uncommitted) records, but the primary uses this position to know when it can advance replication.

**`replica_received_lsn`** = the highest LSN any replica has acknowledged receiving.

Aggregate lag = `primary_durable_lsn - replica_received_lsn`.

#### Replica Side

```rust
pub struct ReplicaShippingState {
    pub wal_received_lsn: Lsn,        // Highest LSN durably received
    pub wal_expected_next_lsn: Lsn,   // Next LSN we expect
}
```

The replica polls segments from the primary and appends them to its local WAL. `wal_received_lsn` advances only after records are validated and durably written to replica's local storage.

#### Invariants

1. **Monotonicity:** Both `shipped_lsn_by_replica` and `wal_received_lsn` are monotonically non-decreasing.
2. **Contiguity:** No LSN is acknowledged unless all prior LSNs in the chain are also acknowledged.
3. **Durable truth:** `wal_received_lsn` is not advanced until records are in durable replica storage.

### 3. Replica Backpressure Protocol

If a replica falls behind (e.g., network lag, slow disk), it can signal the primary to re-ship earlier segments.

#### Backpressure Request

```rust
pub struct ShippingBackpressureRequest {
    pub replica_received_lsn: Lsn,       // What I have received
    pub replica_expected_next_lsn: Lsn,  // What I expect next
}
```

**Trigger:** Replica's receiving loop detects a gap (e.g., expected LSN 50 but got LSN 55).

**Action:** Replica sends backpressure request to primary with `received_lsn=49, expected_next_lsn=50`.

**Primary response:** Find the segment covering LSN 50-54 and re-ship it to the replica.

**Replica behavior:** Validate the re-shipped segment, detect it as a replay (LSN already received), and discard or re-validate.

**Trace event:** Both gap detection and replay are emitted as telemetry for operator alerting.

### 4. Fencing Decision Point

At the primary, a **fencing event** occurs when replication fails (connection lost, checksum mismatch, chain gap). The decision to fence depends on the **quorum policy**.

#### Fencing Policy

```rust
pub enum FencingPolicy {
    Asynchronous,    // Single replica: continue writing
    QuorumEnforced,  // 2+ replicas: block until quorum acks
}

pub fn decide_fencing(event: FencingEvent, policy: FencingPolicy) -> bool {
    // true = block visibility; false = continue
    match policy {
        FencingPolicy::Asynchronous => false,      // Async: continue
        FencingPolicy::QuorumEnforced => true,      // Quorum: block
    }
}
```

**Selection:** Policy is chosen based on replica count:
- 1 replica → Asynchronous (optimize for availability).
- 2+ replicas → QuorumEnforced (optimize for consistency).

#### Fencing Events

| Event | Cause | Replica Action | Primary Trace |
|-------|-------|-----------------|---------------|
| `ReplicaConnectionLost` | QUIC connection dropped | N/A | Emit; decide fencing |
| `ReplicaChecksumMismatch` | Received segment ≠ computed | Reject segment; fence | Emit; decide fencing |
| `ReplicaChainGap` | LSN chain broken (gap) | Emit backpressure | Emit; decide fencing |
| `ReplicaUnknownError` | Any other error | Fence | Emit; decide fencing |

#### Fencing Semantics

- **Asynchronous mode:** Connection lost → emit trace → continue shipping to other replicas. No transaction block.
- **Quorum mode:** Connection lost → emit trace → block new transaction visibility until promoted replica recovers _or_ demotion is triggered.

**Important:** The quorum consensus algorithm (F3) owns the actual block/unblock logic. F1 only defines the decision point and trace events.

### 5. No Silent Failures

Every error is auditable:

1. **Checksum mismatch** → `ReplicaChecksumMismatch` trace + fencing decision.
2. **Chain gap** → `ReplicaChainGap` trace + fencing decision + backpressure request.
3. **Connection loss** → `ReplicaConnectionLost` trace + fencing decision.
4. **Validation error** → `RecordSelfInvalid` (from `WalShipmentBatch::validate()`).

All trace events include:
- Timestamp
- Event type
- Replica ID
- LSN range (if applicable)
- Fencing decision (Asynchronous/QuorumEnforced)

### 6. Protocol Layering

```
┌─ Application Plane ──────────────────────────────────────┐
│  (Transactions, visibility, commit)                       │
└──────────────────────┬──────────────────────────────────┘
                       │
┌─ Quorum Consensus (F3) ──────────────────────────────────┐
│  (Promotion, fencing token, replication mode)            │
└──────────────────────┬──────────────────────────────────┘
                       │
┌─ Fencing Layer (F1) ─────────────────────────────────────┐
│  (Fencing decision, event tracing, policy selection)     │
└──────────────────────┬──────────────────────────────────┘
                       │
┌─ Shipping Layer (F1) ────────────────────────────────────┐
│  (Segment shipping, checksum, LSN correlation)           │
└──────────────────────┬──────────────────────────────────┘
                       │
┌─ WAL Batch Layer ────────────────────────────────────────┐
│  (WalShipmentBatch validation, chain check)              │
└──────────────────────┬──────────────────────────────────┘
                       │
┌─ QUIC Transport (D1/D5) ─────────────────────────────────┐
│  (Frames, streams, error handling, connection lifecycle) │
└──────────────────────────────────────────────────────────┘
```

Each layer is independent and testable. F1 sits between Quorum Consensus and WAL Batch validation.

## Design Scope

### In scope (F1)

✓ Segment shipping protocol (identity, range, checksum, validation).  
✓ LSN correlation model (shipped_lsn, received_lsn, lag tracking).  
✓ Replica backpressure (request-replay protocol).  
✓ Fencing decision point (connection loss → quorum policy check).  
✓ Trace event definitions and semantics.  
✓ Pure decision functions (no async, no QUIC I/O).  
✓ Test scenarios (4 concrete tests + bonus tests).

### Out of scope (deferred)

✗ Actual QUIC stream I/O (use mock in tests).  
✗ Quorum consensus algorithm (F3).  
✗ Compression/encryption (future).  
✗ Replication history compaction (future).  
✗ PITR segment replay scheduling (future, for backup/recovery).

## Doctrinal Compliance

| Doctrine | Compliance |
|----------|-----------|
| **Durability-before-visibility** | Segment shipping condition enforces `end_lsn <= durable_lsn` |
| **RAM is never truth** | Shipped segments are pre-existing durable records; no mutation |
| **LSN chain contiguity** | Replica validates via `WalShipmentBatch::validate()` |
| **Cryptographic integrity** | Checksum validation at replica; mismatch triggers fencing |
| **No silent failures** | All errors emit trace events and decision points |
| **Single-primary topology** | Only Primary role ships; only Replica role receives |
| **Deterministic behavior** | Segment creation, checksum, validation are pure functions |

## Invariants Preserved

1. **Segment durability:** No segment is shipped until durable at primary.
2. **Replica validation:** Replicas validate structure, checksum, and LSN chain.
3. **LSN monotonicity:** shipped_lsn and received_lsn only increase.
4. **Fencing decision:** Explicit policy-based (async vs. quorum).
5. **Audit trail:** Every error and decision is traceable.

## Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| **Checksum collision** (rare but possible) | Use FNV-1a 64-bit; future decision can upgrade to SHA256 if needed. Collisions detected by replica applying wrong state. |
| **Replica lag exceeds primary capacity** | Backpressure request allows replay; operator can adjust replication mode or promote. |
| **Connection flapping** | Backoff in D5 (QUIC stream manager); F1 just emits events. |
| **Quorum loss** | F3 quorum algorithm handles promotion/demotion; F1 provides trace events. |
| **Replica diverges** | Chain gap detection + fencing triggers safe recovery path. |

## Integration Points

### F1 → D3 (mTLS Identity)

The shipping envelope is sent over a QUIC stream authenticated by D3's certificate identity. F1 does not validate identity; D3 ensures the replica is authorized before F1 runs.

### F1 → F3 (Quorum Consensus)

F1 defines the fencing decision point. F3 uses fencing events to decide promotions, demotions, and when to block visibility.

### F1 → D5 (QUIC Stream Manager)

F1 defines the protocol (segment envelope); D5 implements backpressure, retry logic, and async wiring.

### F1 → WAL Batch Validation

F1 calls `WalShipmentBatch::validate()` to check LSN chain. This is the replica's defense against forks and reordering.

## Testing Strategy

### 4 Required Tests

1. **`test_shipping_thread_reads_committed_segments`**
   - Create committed segment (LSN 1-5, durable).
   - Build ShippingSegmentEnvelope.
   - Validate structure and checksum.
   - Check shipping condition.

2. **`test_replica_receives_validates_appends`**
   - Replica receives ShippingSegmentEnvelope (LSN 6-10).
   - Validate structure and checksum.
   - Validate via WalShipmentBatch (LSN chain check).
   - Confirm next_expected_lsn advances to 11.

3. **`test_shipping_backpressure_handles_replica_lag`**
   - Primary ships up to LSN 50.
   - Replica receives only LSN 1-20 (lag=30).
   - Replica emits backpressure request (received=20, expected=21).
   - Primary re-ships segment starting at LSN 21.

4. **`test_shipping_detects_fencing_on_connection_loss`**
   - Async mode (1 replica): connection lost → no fence.
   - Quorum mode (2+ replicas): connection lost → fence.
   - Verify trace event is emitted.

### Bonus Tests

- LSN correlation state tracking (update, lag computation).
- Segment checksum determinism.
- Corrupted segment fails validation.

## Validation

### Compilation

```powershell
cargo check -p andromeda-storage --all-features
cargo check --workspace
```

### Tests

```powershell
cargo test -p andromeda-storage --lib hadr::shipping_runtime --quiet
cargo test -p andromeda-storage --quiet
```

### Doctrine Compliance

- [x] No unsafe code.
- [x] No gRPC, no tonic.
- [x] No runtime JSON defaults.
- [x] No ad hoc SQL.
- [x] Pure decision functions (testable without async/QUIC).
- [x] Deterministic behavior (checksums, validation).

## Follow-up Work

- **F1-IMPL:** Async wiring, QUIC integration, thread model.
- **D5:** QUIC stream manager (backpressure, retry).
- **F2:** SafeStart mode (replica applies received records immediately).
- **F3:** Quorum consensus (promotion, fencing token enforcement).
- **F4:** PITR segment replay (for backup/restore).

## Decision Artifacts

| Artifact | Location |
|----------|----------|
| **Shipping Runtime Types** | `crates/andromeda-storage/src/hadr/shipping_runtime.rs` |
| **Test Suite** | Same file, `#[cfg(test)]` module |
| **Integration** | `crates/andromeda-storage/src/hadr.rs` (pub use) |

## Approval Checklist

- [x] Design covers all 4 required test scenarios.
- [x] LSN correlation is explicit and auditable.
- [x] Fencing decision point is clear and policy-driven.
- [x] Doctrine compliance verified (durability, determinism, no silent failures).
- [x] No dependency on QUIC runtime (mock in tests).
- [x] No quorum consensus implementation (decision point only).
- [x] All errors emit trace events.
- [x] Integration points to D3, F3, D5, and WAL layer are clear.

## Appendix: Example Scenario

**Setup:** Primary (node 1) with replicas (nodes 2, 3). Quorum mode.

**Timeline:**

| Time | Event | State |
|------|-------|-------|
| T0 | Primary writes LSN 1-50, durable. | durable_lsn=50 |
| T1 | ShippingThread ships segment [1-10] to both replicas. | shipped_lsn[2]=10, shipped_lsn[3]=10 |
| T2 | Replica 2 receives, validates, appends segment [1-10]. | replica2.wal_received_lsn=10 |
| T3 | Replica 3 network lag; only receives [1-5]. | replica3.wal_received_lsn=5 |
| T4 | Primary loses connection to Replica 3. | FencingEvent::ReplicaConnectionLost |
| T5 | Fencing policy = QuorumEnforced (2 replicas). | decide_fencing() returns true (fence) |
| T6 | Primary blocks new transaction visibility. | Transaction queue paused |
| T7 | Replica 3 reconnects; emits backpressure (received=5, expected=6). | Backpressure request sent |
| T8 | Primary ships segment [6-20] to Replica 3. | Replica 3 receives and validates |
| T9 | Replica 3 receives LSN up to 20. | replica3.wal_received_lsn=20 |
| T10 | Primary unblocks (quorum restored). | New transactions visible |

**Trace events emitted:** `ReplicaConnectionLost` (T4), `[BackpressureRequest]` (T7), `[SegmentShipped]` (T8), `[SegmentReceived]` (T9).

---

**Status:** Specification Complete. Ready for F1-IMPL.
