# DEC-020: F3 — Quorum Runtime Sequencing

## Status

**Designed and Implemented**. Core modules completed; integration with F1 (shipping), D3 (mTLS identity), D4 (executor bridge), and promotion voting (quorum.rs) validated by 18+ test scenarios.

---

## Context

Andromeda V0 HA/DR foundation has progressed through:
- **DEC-019 (F1):** WAL shipping runtime model (segments, LSN correlation, fencing events).
- **DEC-018 (D3):** mTLS replica identity extraction for secure QUIC connections.
- **D4:** HA/DR surface plane executor and listener infrastructure.
- **D2:** Per-plane listener contracts and coupling to shipping.

The next layer is the **quorum runtime sequencing**: the decision engine that orchestrates WAL shipping decisions and replica promotion eligibility in the single-primary topology. This decision specifies the membership model, write admission control, promotion ranking algorithm, and fencing policy that guide replica coordination.

---

## Problem Statement

The quorum runtime must coordinate:
1. **Replica Membership:** Track connectivity state (Alive, Suspect, Dead) with membership epoch.
2. **Write Admission:** Require min quorum ACK before commit visibility (configurable per replication mode).
3. **Promotion Eligibility:** Rank replicas by LSN distance; deterministically select best candidate.
4. **Fencing Decision:** Pure function mapping (membership, LSN, policy) → Block/Allow.
5. **Failure Handling:** Connection loss triggers membership re-election; block writes until quorum reformed.
6. **No Byzantine Quorum:** V0 assumes benign failures only.

The design must:
- Ensure all decision logic is **pure** (deterministic, replay-safe, no I/O).
- Maintain **membership epoch** monotonicity (incremented on every topology change).
- Make **write admission control** explicit (async vs quorum mode).
- Support **deterministic leader election** (LSN distance + ID tie-break).
- Produce **auditable state machines** (replica lifecycle: Initial → Active → Suspect → Removed → Promoted).
- Integrate seamlessly with F1 (shipping), D3 (identity), and existing promotion voting (quorum.rs).

---

## Decision

### 1. Quorum Membership Model

**Core Types:**

```rust
pub struct ReplicaMember {
    pub replica_id: u64,
    pub health_state: ReplicaHealthState,      // Alive, Suspect, Dead
    pub received_lsn: Lsn,                     // Durable LSN at replica
    pub shipped_lsn: Lsn,                      // What primary has sent
}

pub enum ReplicaHealthState {
    Alive,      // Connected and responsive
    Suspect,    // Missed heartbeat(s)
    Dead,       // Connection lost or unresponsive
}

pub struct QuorumMembership {
    members: HashMap<u64, ReplicaMember>,
    epoch: u64,                                // Incremented on topology change
    primary_id: u64,
    quorum_size: usize,                        // floor(N/2) + 1
}
```

**Invariants:**
- `epoch` strictly monotonically increases on every topology change.
- `quorum_size = floor(members.len() / 2) + 1` (majority).
- No replica ID can equal primary ID.
- No duplicate replica IDs in membership.
- Membership is **durable** (not runtime-only); persisted in catalog.

**Lifecycle Operations:**
- `mark_suspect(replica_id)` — transitions Alive → Suspect, increments epoch.
- `mark_alive(replica_id)` — transitions Suspect → Alive, increments epoch.
- `mark_dead(replica_id)` — transitions Alive/Suspect → Dead, increments epoch.
- `remove_dead(replica_id)` — removes dead replica, recomputes quorum_size, increments epoch.
- `update_replica_lsn(replica_id, received_lsn, shipped_lsn)` — updates LSN state.

### 2. Replica State Machine

**State Lifecycle:**

```
Initial
  ↓ (HeartbeatReceived)
Active
  ├─ (HeartbeatMissed) → Suspect
  ├─ (LsnAdvanced) → Active (no-op, stays active)
  ├─ (PromotionStaged) → Candidate
  └─ (RemovalRequested) → Removed

Suspect
  ├─ (HeartbeatReceived) → Active
  ├─ (DeadThresholdExceeded) → Removed
  └─ (RemovalRequested) → Removed

Candidate
  ├─ (PromotionSucceeded) → Promoted
  ├─ (PromotionFailed) → Active
  └─ (RemovalRequested) → Removed

Removed (terminal)
Promoted (terminal)
```

**State Tracker:**
- `MembershipStateTracker` records full transition history for audit.
- Every transition is atomic and deterministic.
- Pure function `transition(current, event) → Result<next, Error>`.

### 3. Write Admission Control

**Replication Modes:**

```rust
pub enum ReplicationMode {
    Asynchronous,       // No ACK required; writes visible immediately
    QuorumEnforced,     // Must have min quorum of ACKs before visibility
}

pub struct QuorumConsensus {
    pub min_quorum_acks: usize,    // 0 for async, quorum_size for quorum mode
}

impl QuorumConsensus {
    pub fn can_admit_write(&self, alive_acks: usize) -> bool {
        alive_acks >= self.min_quorum_acks
    }
}
```

**Write Admission Flow:**
1. Primary receives write from client.
2. If async mode: write is visible immediately (min_quorum_acks = 0).
3. If quorum mode: write is held until min_quorum_acks replicas ACK.
4. Fencing decision (below) may block further writes if quorum is lost.

**Contract:**
- Async mode: write visibility is immediate; fencing depends on policy.
- Quorum mode: write visibility requires min quorum of alive replica ACKs.

### 4. Promotion Eligibility and Ranking

**Ranking Algorithm:**

```rust
pub struct PromotionEligibility {
    pub replica_id: u64,
    pub lsn_distance: i64,        // primary.durable_lsn - replica.received_lsn
    pub is_eligible: bool,         // Alive AND lsn_distance <= 0
}

fn rank_promotion_candidates(
    membership: &QuorumMembership,
    primary_durable_lsn: Lsn,
) -> Vec<PromotionEligibility> {
    // Compute rank for each alive replica
}

fn select_promotion_candidate(
    membership: &QuorumMembership,
    primary_durable_lsn: Lsn,
) -> Option<PromotionEligibility> {
    // Return best candidate (shortest LSN distance, lowest ID on tie)
}
```

**Ranking Rules:**
1. **Eligibility:** Replica must be Alive AND `received_lsn >= primary.durable_lsn`.
2. **Distance Metric:** `lsn_distance = primary.durable_lsn - replica.received_lsn`.
   - 0 = fully caught up (best).
   - Positive = lagging (worse candidate).
3. **Tie-Break:** On equal distance, prefer lower replica ID (stable, deterministic).
4. **Determinism:** Sorted by distance then ID; same input always produces same ranking.

**Integration with quorum.rs:**
- `evaluate_promotion()` uses quorum_runtime results indirectly.
- Caller can use `select_promotion_candidate()` to determine which replica to stage.
- Promotion voting (existing `HadrPromotionRequest`) validates fencing context.

### 5. Fencing Decision Point

**Fencing Policy:**

```rust
pub enum FencingPolicy {
    Allow,                  // Never block writes on replica failure
    BlockOnQuorumLoss,      // Block if membership loses quorum
}

pub enum FencingEvent {
    ReplicaDisconnected,    // Connection lost
    ReplicaChecksumMismatch, // Validation failure
    ReplicaLsnGap,          // Gap detected in chain
    Unknown,                // Unclassified failure
}

pub enum FencingDecision {
    Allow,                  // Writes may proceed
    Block,                  // Writes must be held
}

pub fn decide_fencing(
    membership: &QuorumMembership,
    event: FencingEvent,
    policy: FencingPolicy,
    replication_mode: ReplicationMode,
) -> FencingDecision {
    // Pure function: returns Allow or Block deterministically
}
```

**Fencing Logic:**

| Policy | Mode | Quorum Status | Decision |
|--------|------|---------------|----------|
| Allow | Any | Any | Allow |
| BlockOnQuorumLoss | Async | Lost | **Allow** (async ignores quorum) |
| BlockOnQuorumLoss | Async | Maintained | Allow |
| BlockOnQuorumLoss | Quorum | Lost | **Block** |
| BlockOnQuorumLoss | Quorum | Maintained | Allow |

**Semantics:**
- Async mode: fencing policy is advisory only; quorum status does not block writes.
- Quorum mode + BlockOnQuorumLoss: missing quorum blocks all new writes.
- Fencing decisions are **deterministic** and **replay-safe**.

### 6. Deterministic Consensus

**Pure Functions:**
All decision functions are pure (no I/O, no time dependency):

1. `transition(state, event) → Result<next_state>` — Deterministic state machine.
2. `decide_fencing(membership, event, policy, mode) → FencingDecision` — Deterministic.
3. `rank_promotion_candidates(membership, durable_lsn) → Vec<Rank>` — Deterministic.
4. `select_promotion_candidate(membership, durable_lsn) → Option<Rank>` — Deterministic.

**Replay Safety:**
- Given identical membership snapshots and LSN states, decisions are identical.
- State machine history is fully auditable; operators can replay decisions offline.
- No hidden state; all inputs are explicit function parameters.

---

## Design Scope

### In Scope ✓

- [x] **Membership tracking** (Alive, Suspect, Dead states) with health state transitions.
- [x] **Membership epoch** monotonicity and topology change tracking.
- [x] **Write admission control** (async vs quorum mode with min ACK thresholds).
- [x] **Promotion eligibility** ranking by LSN distance and replica ID tie-break.
- [x] **Fencing decision** pure function (membership + policy + mode → Block/Allow).
- [x] **State machine** (Initial → Active → Suspect → Removed/Promoted) with transition history.
- [x] **Pure functions** (no I/O, deterministic, replay-safe).
- [x] **Integration contracts** with F1 (shipping), D3 (identity), D4 (executor).
- [x] **Test coverage** (18+ scenarios across membership, promotion, fencing).

### Out of Scope ✗

- [ ] **Actual network I/O** (QUIC, TCP). Module assumes caller orchestrates connections.
- [ ] **Heartbeat/keepalive protocol** (caller implements via D4 plane listener).
- [ ] **Byzantine-resistant quorum** (V0 assumes benign failures only).
- [ ] **Reconfiguration consensus** (dynamic membership joins/leaves; static for V0).
- [ ] **Snapshots/state transfer** (PITR and WAL replay handle recovery).

---

## Invariants Preserved

### Doctrine Compliance

1. **Durability-before-Visibility:**
   - Promotion eligibility requires `replica.received_lsn >= primary.durable_lsn`.
   - Fencing blocks writes if quorum cannot be reached (in quorum mode).

2. **Cryptographic Integrity:**
   - Fencing events categorize failures (checksum, gap, disconnect).
   - Caller is responsible for validating segments; quorum_runtime validates membership only.

3. **LSN Chain Contiguity:**
   - LSN ranking ensures promoted replica does not lose committed state.
   - Gap detection triggers fencing event (caller processes via shipping_runtime).

4. **Deterministic State:**
   - Membership snapshots are immutable; epochs track changes explicitly.
   - All decisions are pure functions (replay-safe).

5. **No Silent Failures:**
   - Every state transition recorded in tracker history.
   - Fencing decisions are explicit (Allow/Block); no default/implicit behavior.

### Epoch Monotonicity

- Initial epoch = 0.
- Every topology change increments epoch by 1.
- Replica observes strict epoch ordering via membership snapshot.
- Promotion voting uses epoch to detect split-brain (existing quorum.rs).

### Quorum Majority

- Membership requires `quorum_size = floor(N / 2) + 1`.
- Write admission requires `alive_acks >= quorum_size` (in quorum mode).
- Fencing blocks writes if `alive_replicas < quorum_size` (in quorum mode).

---

## Integration Points

### F1 (WAL Shipping Runtime)

- **wal_shipped_lsn** tracking: `ReplicaMember.shipped_lsn` updated on segment receipt ACK.
- **wal_received_lsn** tracking: `ReplicaMember.received_lsn` updated on segment durable.
- **Backpressure:** Write admission respects min quorum ACKs via `QuorumConsensus`.
- **Fencing events:** Shipping runtime triggers fencing decisions (disconnect, checksum, gap).

### D3 (mTLS Identity & Replica Identity)

- **Replica identity binding:** `ReplicaMember.replica_id` correlates with mTLS certificate CN.
- **Authentication flow:** D3 establishes identity; quorum_runtime tracks membership.
- **Epoch verification:** Replica's observed epoch compared against membership epoch for split-brain detection.

### D4 (HA/DR Executor & Plane Listener)

- **Heartbeat orchestration:** Listener receives heartbeat; calls `mark_alive()`, `mark_suspect()`, `mark_dead()`.
- **Replica LSN updates:** Listener receives LSN advancement; calls `update_replica_lsn()`.
- **Promotion staging:** Executor uses `select_promotion_candidate()` to pick best candidate for promotion attempt.

### Existing quorum.rs (Promotion Voting)

- **Candidate selection:** quorum_runtime provides `PromotionEligibility`; caller stages `HadrPromotionRequest`.
- **Voting context:** Promotion voting uses existing `HadrQuorumMembership` and `HadrFencingContext`.
- **Epoch consistency:** Both modules respect epoch monotonicity independently.

---

## Testing Strategy

### Test Coverage (18+ scenarios)

1. ✅ **test_quorum_initialized_with_single_primary** — Membership creation, quorum size.
2. ✅ **test_new_replica_joins_membership_on_hello** — Initial → Active state transition.
3. ✅ **test_replica_marked_suspect_on_missed_heartbeat** — Active → Suspect, epoch increment.
4. ✅ **test_quorum_blocks_writes_if_membership_quorum_lost** — Fencing on quorum loss.
5. ✅ **test_replica_promotion_blocked_if_lsn_behind_primary** — Ineligible replica detection.
6. ✅ **test_promotion_allowed_when_replica_durable_lsn_caught_up** — Eligible replica detection.
7. ✅ **test_fencing_decision_blocks_in_quorum_mode_on_disconnect** — Quorum-enforced fencing.
8. ✅ **test_fencing_decision_allows_in_async_mode_on_disconnect** — Async mode ignores quorum.
9. ✅ **test_membership_epoch_increments_on_topology_change** — Epoch monotonicity.
10. ✅ **test_simultaneous_join_and_promote_atomic** — Atomic transitions.
11. ✅ **test_leader_election_deterministic_by_lsn_distance** — Promotion ranking.
12. ✅ **test_orphan_replica_detection_and_removal** — Dead replica lifecycle.
13. ✅ **test_write_admission_async_no_acks_required** — Async consensus.
14. ✅ **test_write_admission_quorum_requires_majority_acks** — Quorum consensus.
15. ✅ **test_replica_lsn_updates_tracked** — LSN state tracking.
16. ✅ **test_consensus_computation_from_membership** — Consensus from majority.
17. ✅ **test_membership_state_tracker_complete_lifecycle** — Full lifecycle replay.
18. ✅ **test_fencing_policy_allow_never_blocks** — Fencing policy behavior.
19. ✅ **test_promotion_eligibility_ranking_complete** — Complete ranking rules.

### Test Methodology

- **Pure function testing:** All decision functions tested in isolation with deterministic inputs.
- **State machine validation:** All valid transitions tested; invalid transitions rejected.
- **Replay-safety:** Transition history is recorded and replayable.
- **Determinism:** Identical inputs produce identical outputs across test runs.
- **Boundary conditions:** Quorum edge cases (tied distances, single replica, all dead).

---

## Risks and Mitigations

| Risk | Severity | Mitigation |
|------|----------|-----------|
| **Membership state drift** | High | Membership is durable catalog record; epoch changes auditable. |
| **Fencing false positives** | Medium | Fencing policy is configurable; can be set to Allow during development. |
| **LSN ranking instability** | Low | Ranking is deterministic (distance + ID); tie-break rules stable. |
| **Promotion attempt race** | Low | Atomic transitions; state machine rejects invalid event sequences. |
| **Byzantine failure** | Not addressed | V0 assumes benign failures only; Byzantine-resistant quorum deferred. |

**Mitigation Rationale:**
- Membership durability ensures state recovery after restart.
- Configurable fencing policy allows gradual adoption of quorum enforcement.
- Pure functions enable offline audit and replay of decisions.
- State machine exhaustiveness ensures no edge cases slip through.

---

## Operational Notes

### Transition Triggers

| Trigger | Handler | Effect |
|---------|---------|--------|
| Heartbeat received | D4 listener | mark_alive() (Suspect → Active) or no-op (Active → Active) |
| Heartbeat missed N times | D4 listener | mark_suspect() (Active → Suspect, epoch++) |
| Dead threshold exceeded | D4 listener | mark_dead() (Alive/Suspect → Dead, epoch++) |
| LSN advanced | D4 listener | update_replica_lsn() |
| Replica disconnected | Shipping thread | decide_fencing() → Block/Allow |
| Checksum mismatch | Shipping thread | decide_fencing(ReplicaChecksumMismatch) |
| LSN gap detected | Shipping thread | decide_fencing(ReplicaLsnGap) |

### Configuration

```rust
// Per-deployment configuration
let quorum_config = QuorumConfig {
    replication_mode: ReplicationMode::QuorumEnforced,
    fencing_policy: FencingPolicy::BlockOnQuorumLoss,
    heartbeat_miss_threshold: 3,         // Mark suspect after 3 misses
    heartbeat_dead_threshold: 10,        // Mark dead after 10 total misses
};
```

### Monitoring & Observability

- **Membership epoch:** Expose as metric; watch for rapid increments (topology churn).
- **Alive/suspect/dead counts:** Dashboard to show quorum health.
- **Promotion attempts:** Log candidate selection (LSN distance, ID).
- **Fencing decisions:** Audit log every Allow/Block decision (replay for forensics).
- **State transition history:** Retention period (e.g., 1 hour) for debugging.

---

## Acceptance Criteria

- ✅ All 18+ test scenarios passing.
- ✅ Fencing decision is deterministic and replay-safe.
- ✅ No writes visible if membership quorum cannot be established (quorum mode).
- ✅ Promotion rank computed correctly (LSN distance, tie-break by replica ID).
- ✅ State transitions are atomic (no race between join and promote).
- ✅ Membership epoch increments on every topology change.
- ✅ No `unsafe_code` (Rust memory safety guaranteed).
- ✅ No async I/O in core logic (pure functions only).
- ✅ Integration contracts with F1, D3, D4 clear and verified.

---

## Validation

### Code Review Checklist

- [ ] Module exports (hadr.rs) include quorum_runtime and membership_transitions.
- [ ] All pure functions have determinism guarantees in docs.
- [ ] Test coverage includes all state transitions and fencing policies.
- [ ] Membership epoch increment validated in tests.
- [ ] Promotion ranking matches specification (LSN distance, tie-break).
- [ ] Fencing decisions match policy matrix above.
- [ ] Integration points with F1, D3, D4 documented in comments.

### Deployment Validation

- [ ] Membership durability verified (catalog record persists across restart).
- [ ] Heartbeat protocol tested (transitions triggered correctly by D4 listener).
- [ ] Fencing decisions observed in shipping logs (Allow/Block decisions logged).
- [ ] Promotion attempt logs show correct candidate selection.
- [ ] Epoch advancement monitored during topology changes.

---

## Future Work

1. **Reconfiguration (V1):** Dynamic membership changes (add/remove replicas) without restart.
2. **Byzantine Quorum (V2):** Extend to Byz quorum if untrusted networks required.
3. **Snapshot Transfer (V2):** State transfer for lagging replicas (fast catch-up).
4. **Weighted Quorum (V1.5):** Allow non-uniform replica weights (e.g., prefer certain replicas).

---

## Appendix: Example Scenario

### Scenario: Replica Failure and Recovery

**Initial State:**
- Primary (ID=1) with 3 replicas (ID=2,3,4).
- Quorum size = 3/2+1 = 2.
- All replicas Alive, caught up (received_lsn = primary.durable_lsn = 1000).
- Epoch = 0.

**Timeline:**

| Time | Event | State Change | Epoch |
|------|-------|--------------|-------|
| t=0 | Primary initialized | 3 replicas Alive | 0 |
| t=1 | Replica 2 misses heartbeat | mark_suspect(2) → Suspect | 1 |
| t=2 | Replica 2 misses again | (no-op, already Suspect) | 1 |
| t=3 | Replica 2 dead threshold | mark_dead(2) → Dead | 2 |
| t=4 | Fencing check | has_quorum()? 2 Alive >= 2? ✓ **Allow** | 2 |
| t=5 | Replica 2 reconnects | mark_alive(2) → Alive | 3 |
| t=6 | Replica 2 caught up | received_lsn = 1000 | 3 |

**Write Admission:**
- t=0 to t=3: Consensus requires 2 ACKs (can_admit_write(3) ✓, can_admit_write(2) ✓).
- t=4: Consensus requires 2 ACKs (can_admit_write(2) ✓).
- t=5+: Consensus requires 2 ACKs (can_admit_write(3) ✓).

**Promotion Ranking (at t=4, replica 2 is Dead):**
- Replica 3: lsn_distance=0, eligible=true, rank=0.
- Replica 4: lsn_distance=0, eligible=true, rank=1 (tie-break: ID 3 < 4).
- Best candidate: Replica 3.

---

## References

- **DEC-019 (F1):** WAL Shipping Runtime Model (segments, LSN correlation, fencing).
- **DEC-018 (D3):** mTLS Identity Extraction.
- **DEC-017:** QUIC Runtime (quinn + rustls).
- **D4:** HA/DR Surface Plane Executor.
- **quorum.rs:** Promotion voting and epoch-based fencing tokens.
