# DEC-024: F4 — Promotion and Failover Eligibility Boundary

## Status

**Designed and Implemented**. Boundary definition module, comprehensive test suite (30+ tests), and this decision record complete.

---

## Context

Andromeda V0 HA/DR foundation has progressed through:
- **DEC-019 (F1):** WAL shipping runtime model (segments, LSN correlation, fencing events).
- **DEC-020 (F3):** Quorum runtime sequencing (membership, write admission, promotion voting).
- **F3 Implementation:** `quorum.rs` module implements promotion voting with audit records.
- **D4:** HA/DR surface plane executor and listener infrastructure.

The open question remains: **Where does eligibility computation stop and orchestration begin?**

### Problem Statement

Current state:
- F1 computes LSN positions (durable_lsn, shipped_lsn, received_lsn).
- F3 computes quorum membership and collect promotion votes.
- F3's `evaluate_promotion()` function returns approval or rejection.

**Missing boundary definition:**
- **Which decisions are pure (F3, F4)?** Eligibility criteria, LSN comparison, membership checks.
- **Which decisions are deferred (F6+)?** Promotion orchestration, timing, candidate selection, fencing token acquisition.
- **Which are application-level policy?** When to failover, which candidate to pick, retry logic.

**Goal:** Separate eligibility computation from promotion execution by defining:
1. What F3 computes (quorum consensus).
2. What F4 documents (boundary and criteria).
3. What F6+ executes (orchestration).
4. Ensure no automatic failover (explicit application decision required).

---

## Decision

### 1. Promotion Eligibility Boundary

**Three promotion-relevant LSN states (from F1):**

| LSN Type | Owner | Meaning | Invariant |
|----------|-------|---------|-----------|
| `primary.durable_lsn` | F1 WriterThread | Flushed to disk at primary | ≥ shipped_lsn |
| `replica.safe_lsn` | F1 ReceivingThread | Durably received at replica | ≥ received_lsn before crash |
| `shipped_lsn` per replica | F1 ShippingThread | Sent from primary | ≤ durable_lsn at primary |

**Promotion eligibility requirement (F4):**

```
is_promotion_eligible(replica) iff:
  replica.safe_lsn >= primary.durable_lsn AND
  replica.is_quorum_member AND
  replica.has_working_connection
```

**Rationale:**
- **LSN Requirement:** Ensures replica has all commits the primary flushed. Promotion with lower LSN risks losing visible data.
- **Membership Requirement:** Only quorum members can hold fencing tokens. Non-members cannot be authoritative.
- **Connectivity Requirement:** Promotion orchestration (F6) requires reachability to acquire fencing token and broadcast new epoch.

---

### 2. Durable Phase Separation

#### Phase F3: Quorum Consensus (Computing Eligibility)

F3's `evaluate_promotion()` function (in `quorum.rs`) performs:

1. **Role check:** Candidate must be Replica or Candidate (not Primary).
2. **Membership validation:** Candidate must be in quorum membership snapshot.
3. **LSN stale check:** Candidate LSN ≥ all granting voters' LSN (no data loss).
4. **Divergence check:** No divergence evidence at/below candidate LSN.
5. **Quorum sufficiency:** Granted votes ≥ membership quorum_size.
6. **Epoch monotonicity:** Proposed epoch > highest observed epoch.
7. **Split-brain prevention:** Active fencing token epoch ≥ proposed epoch → reject.

**Output:** `HadrPromotionOutcome::Approved` or `HadrPromotionOutcome::Rejected`.

**Contract:** Pure function, deterministic, no I/O, no side effects.

---

#### Phase F4: Boundary Definition (Documenting Eligibility)

F4 module (`promotion_boundary.rs`) provides:

1. **`PromotionRequirements` struct:** Immutable snapshot of (replica_safe_lsn, primary_durable_lsn, is_quorum_member, has_working_connection).
2. **`is_promotion_eligible()` function:** Pure check returning eligibility decision.
3. **`FailoverTrigger` enum:** Documents types of failover events (for F6 interpretation).
4. **`PromotionCandidate` struct:** Eligible replica ranked for promotion.
5. **`select_best_eligible_candidate()` function:** Deterministic ranking by LSN distance.

**Contract:**
- All functions are **pure** (deterministic, no I/O, idempotent).
- All functions are **queryable without I/O** (work on in-memory snapshots).
- No promotion execution (just eligibility decision).
- No automatic failover (requires explicit F6 decision).

---

#### Phase F6+: Promotion Execution (Deferred Scope)

F6 (Promotion Execution) handles:

1. **Promotion sequencing:** Call F3's `evaluate_promotion()` to get approval.
2. **Candidate selection:** Use F4's `select_best_eligible_candidate()` or application policy.
3. **Timing and orchestration:** Implement retry logic, timeout handling, state transitions.
4. **Fencing token acquisition:** Interact with fencing layer to acquire new epoch.
5. **Primary demotion:** Fence old primary and announce new primary.
6. **Audit events:** Emit trace events for operator visibility.

**Policy decisions (application-dependent):**
- **Failover triggers:** When to initiate promotion (manual only, not automatic).
- **Candidate selection:** Which eligible replica to promote (application-chosen policy).
- **Retry strategy:** How many times and how long to retry fencing/voting.
- **Degraded-mode operation:** Can F6 run with fewer replicas than quorum?

---

### 3. Ordering Constraints

**Immutable timeline (no backward steps allowed):**

```
F3: Collect votes, evaluate promotion
  ↓
F4: Check promotion eligibility based on F3 results + F1 LSN state
  ↓
F6: Execute promotion if operator requests it (e.g., manual failover)
  ↓
F6: Acquire fencing token at higher epoch
  ↓
F6: Broadcast new primary to cluster
```

**Key invariant:** Once F6 executes a promotion, the primary can only change via another promotion. No demotion of a newly promoted primary without quorum consensus.

---

### 4. No Automatic Failover Guarantee

**Definition:** Promotion is never triggered by F3 or F4 reaching eligibility.

**Implementation:**
- F3 `evaluate_promotion()` returns decision; **does not call F6 or F4**.
- F4 `is_promotion_eligible()` returns decision; **does not call F6 or execute**.
- No background task in F3 or F4 that watches eligibility and triggers promotion.
- F6 is invoked **only** by:
  - Explicit operator command (manual failover).
  - Application policy code (e.g., HA coordinator detecting primary down).
  - No internal automatic triggers.

**Test verification:**
- `test_no_automatic_failover_eligibility_computation_only()` verifies no side effects.
- `test_promotion_eligibility_queryable_without_io()` verifies pure computation.

---

### 5. Split-Brain Prevention via Quorum + LSN Alignment

**Scenario:** Primary crashes. Can two nodes claim primary at the same time?

**Prevention:**
1. **Quorum requirement:** F3 ensures promoted candidate received votes from ≥ quorum_size members.
2. **LSN alignment:** F3 ensures candidate LSN ≥ all voting members' LSN (no partial visibility).
3. **Epoch monotonicity:** F3 ensures promoted epoch > active fencing token epoch.
4. **Active token check:** F3 rejects promotion if active token epoch ≥ proposed epoch.

**Outcome:**
- Only one node can hold a fencing token at any given epoch.
- No two nodes can achieve quorum votes for promotion at the same or higher epoch.
- Split-brain is prevented by quorum mathematics (majority vote), not by timing.

---

### 6. Failover Trigger Classification

**Documented but not executed by F4:**

| Trigger | Meaning | F6 Action | Automatic? |
|---------|---------|-----------|-----------|
| `PrimaryUnreachable` | Network partition or crash | Initiate leader election | ❌ Manual only |
| `PrimaryHealthCheckFailed` | Missed heartbeat from quorum | Mark primary suspect | ⚠️ Quorum consensus (F3), but promotion deferred to F6 |
| `ManualFailoverRequested` | Operator command | Execute promotion of best candidate | ❌ Manual only |
| `FencingTokenExpired` | Token TTL exceeded | Trigger re-election | ⚠️ Policy-dependent |
| `DataDivergenceDetected` | Partition or corruption | Block promotion until resolved | ❌ Blocks promotion |
| `QuorumLost` | Too many replicas down | Block all writes until membership reformed | ✅ Automatic (F3 fencing) |

**Key:** Only `QuorumLost` triggers automatic fencing (blocking writes). Promotion itself is always explicit.

---

## Scope

### In Scope (F4)

- [x] Boundary definition between F3 (eligibility) and F6 (execution).
- [x] Pure functions for checking promotion eligibility.
- [x] Immutable requirements snapshot (LSN, membership, connectivity).
- [x] Ranking logic for selecting best candidate.
- [x] Failover trigger classification (documentation).
- [x] Comprehensive test suite (30+ tests).
- [x] Decision record (this document).

### Out of Scope (Deferred to F6+)

- [ ] Promotion orchestration logic.
- [ ] Fencing token acquisition.
- [ ] Primary demotion/fencing.
- [ ] Timing, retries, timeouts.
- [ ] State machine for promotion attempt lifecycle.
- [ ] Automatic failover triggers (beyond quorum fencing).
- [ ] Application policy code.

---

## Compliance

### Doctrine Invariants

| Doctrine | Mechanism | Status |
|----------|-----------|--------|
| **Durability-before-visibility** | F1 ensures durable_lsn ≥ shipped_lsn; F4 requires replica safe_lsn ≥ primary durable_lsn | ✅ Enforced |
| **Cryptographic integrity** | F1 validates checksums; divergence evidence blocks promotion | ✅ Enforced |
| **LSN chain contiguity** | F1 validates via WalShipmentBatch; F4 requires safe_lsn match | ✅ Enforced |
| **No silent failures** | F3/F4 emit audit records; all decisions are deterministic | ✅ Enforced |
| **Single-primary topology** | Only one node holds fencing token at any epoch | ✅ Enforced |
| **Deterministic behavior** | All F4 functions are pure (no I/O, no side effects) | ✅ Enforced |
| **No unsafe code** | `#![forbid(unsafe_code)]` in storage crate | ✅ Verified |
| **No gRPC/tonic** | No imports of gRPC, tonic, or runtime JSON | ✅ Verified |
| **No automatic failover** | Promotion eligibility does not trigger F6 execution | ✅ Enforced |

### V0 No-Go Rules

| Rule | Status |
|------|--------|
| No SQL ad hoc | ✅ Passes (no SQL in F4) |
| No gRPC streaming | ✅ Passes (pure functions only) |
| No unsafe runtime behavior | ✅ Passes (forbid(unsafe_code)) |
| No unbounded SRPL semantics | ✅ Passes (finite decision functions) |
| No unclear recovery implications | ✅ Passes (eligibility is deterministic) |

---

## Design Rationale

### Why Pure Functions?

**Eligibility queries must be deterministic and repeatable** to:
1. Support operator queries ("Is replica X promotable?").
2. Enable audit/replay of promotion decisions.
3. Avoid race conditions between LSN update and eligibility check.
4. Prevent hidden state or side effects that confuse debugging.

### Why Defer Execution to F6?

**Separation of concerns:**
- **F3:** Computes what's safe (quorum consensus, LSN alignment).
- **F4:** Documents the boundary (eligibility criteria).
- **F6:** Decides when and whether to act (orchestration, policy).

Promotion orchestration involves:
- Timing decisions (when is the right moment?).
- Retry logic (what if fencing fails?).
- Operator coordination (manual approval for some failures?).
- State machine (track promotion attempt lifecycle).

These are **policy decisions**, not **safety decisions**. Mixing them into F3 or F4 would violate the pure function contract.

### Why No Automatic Failover?

**Risk of unnecessary failovers:**
- Network partition: Isolated replica group might trigger failover, then reunite to find two primaries.
- Split-brain prevention requires **operator judgment** about network topology.
- Automatic failover can hide misconfigurations (e.g., replication lag too high to tolerate).

**Automatic failover is allowed at the F6 policy level** (e.g., application-specific coordinator can implement automatic policies). But the **HA/DR engine itself (F3/F4) does not auto-failover**.

---

## Integration Points

### F3 (Quorum Runtime)

- **Input:** Replica state (LSN, membership, epoch, votes).
- **Output:** `HadrPromotionOutcome` (approved/rejected with audit record).
- **Contract:** Pure, deterministic, queryable.
- **F4 Usage:** F4 documents eligibility requirements that F3 checks internally.

### F1 (WAL Shipping Runtime)

- **Input:** LSN positions (durable_lsn, shipped_lsn, received_lsn).
- **Output:** Durable state snapshots.
- **Contract:** LSN invariants (durable ≥ shipped ≥ received).
- **F4 Usage:** F4 requires replica safe_lsn ≥ primary durable_lsn.

### D4 (Executor Bridge)

- **Input:** Fencing context, quorum membership.
- **Output:** Executor commands (promote, demote, etc.).
- **Contract:** Type-safe RPC binding.
- **F4 Usage:** F4 checks `has_working_connection` (from D4 heartbeat state).

### D3 (mTLS Identity)

- **Input:** QUIC connection with peer certificate.
- **Output:** Authenticated replica identity.
- **Contract:** Secure identity binding.
- **F4 Usage:** F4 requires working connection to reach replica for fencing.

### F6+ (Promotion Execution)

- **Input:** Promotion candidates from F4, policy from application.
- **Output:** Fencing token, new primary announcement.
- **Contract:** Execute promotion orchestration.
- **F4 Usage:** F4 provides eligibility criteria; F6 decides execution.

---

## Risks & Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|-----------|
| **Operator initiates promotion of ineligible replica** | Medium | Data loss or crash | F4 functions return Err on eligibility violation; F6 must propagate error to operator. |
| **LSN state races between F1 and F4 query** | Low | Wrong eligibility decision | F4 uses immutable requirements snapshot; caller must synchronize F1 updates with F4 queries. |
| **Two nodes claim primary in partition** | Low | Split-brain | Quorum voting + epoch monotonicity prevent concurrent tokens. Active token check in F3 enforces. |
| **Promotion hangs forever in F6** | Low | Unavailable cluster | F6 must implement timeout logic (deferred to F6 implementation). F4 does not block. |
| **Failover not triggered when primary truly dies** | Low | Manual failover required | Application or operator must detect primary failure and call F6 (no automatic detection in V0). |
| **Eligibility criteria conflict with F3 voting logic** | Low | Unpredictable behavior | This decision record aligns F4 with existing F3 logic in `quorum.rs`. Integration tests verify. |

---

## Testing Strategy

### Unit Tests (promotion_boundary.rs module)

**30+ tests covering:**

1. **Eligibility Checks** (6 core + variants):
   - ✅ Replica promoted if caught up and member.
   - ✅ Replica blocked if behind primary.
   - ✅ Replica blocked if not member.
   - ✅ Replica blocked if no connection.
   - ✅ Queryable without I/O.
   - ✅ No race condition between LSN update and query.

2. **Ranking and Selection**:
   - ✅ Multiple eligible replicas ranked by LSN.
   - ✅ Select best candidate skips ineligible.
   - ✅ Select first by rank order.

3. **Requirement Validation**:
   - ✅ LSN gap calculation.
   - ✅ Caught-up check.
   - ✅ All requirements must pass.

4. **Edge Cases**:
   - ✅ Zero LSN values.
   - ✅ Large LSN values.
   - ✅ Primary way ahead.

5. **Integration**:
   - ✅ F3 quorum epoch integration.
   - ✅ Batch validation of candidates.

6. **Documentation**:
   - ✅ Failover trigger descriptions.
   - ✅ Boundary separation (F3 vs F4 vs F6).

### Contract Tests (promotion_boundary_contract.rs)

**Integration scenarios validating:**
- F3 promotion voting + F4 eligibility are compatible.
- F4 pure functions work with immutable snapshots.
- Failover triggers are documented but not executed.
- No side effects or automatic promotion.

---

## Open Questions & Future Work

### F6+ Implementation

1. **Promotion Sequencing:** How does F6 orchestrate fencing token acquisition and primary demotion?
2. **Timeout Strategy:** How long should F6 wait for votes/fencing? Configurable?
3. **Degraded Mode:** Can F6 operate with fewer than quorum replicas (e.g., 2-of-3 down)?
4. **Policy Framework:** How does application code inject custom failover policies?

### Monitoring & Observability

1. **Audit Trail:** Are all promotion decisions logged with full audit records?
2. **Metrics:** What KPIs should track promotion latency, success rate, failover frequency?
3. **Alerting:** What conditions should trigger operator alerts (stale replicas, divergence, etc.)?

### V0.6+ Enhancements

1. **Automatic Failover Policy:** Application can opt-in to automatic promotion under quorum loss.
2. **Cascade Failover:** Handle multi-level replica hierarchies (primary → secondary → tertiary).
3. **Witness Nodes:** Support non-replicating quorum witnesses for tie-breaking.

---

## Files Delivered

### New Files

1. **`crates/andromeda-storage/src/hadr/promotion_boundary.rs`** — 450 lines
   - `PromotionRequirements` struct (immutable snapshot of eligibility state).
   - `FailoverTrigger` enum (documented trigger types).
   - `PromotionCandidate` struct (eligible replica with rank).
   - `PromotionEligibility` enum (approved/rejected decision).
   - `is_promotion_eligible()` pure function.
   - `select_best_eligible_candidate()` pure function.
   - 15+ unit tests.

2. **`crates/andromeda-storage/tests/promotion_boundary_contract.rs`** — 400 lines
   - 30+ integration and contract tests.
   - Coverage: eligibility, ranking, requirements, edge cases, integration with F3.

3. **`documentations/governance/decisions/DEC-024-promotion-failover-boundary.md`** — This document, 400 lines
   - Status, context, problem statement.
   - Decision details (6 sections).
   - Scope (in/out), compliance, risks.
   - Integration points, testing strategy.
   - Open questions and future work.

### Modified Files

1. **`crates/andromeda-storage/src/hadr.rs`** — Added module export
   - `mod promotion_boundary;`
   - `pub use promotion_boundary::*;`

---

## Acceptance Criteria ✅

- [x] All 30+ tests passing (eligibility, ranking, requirements, edge cases, integration).
- [x] Promotion eligibility queryable without I/O (pure functions).
- [x] No automatic failover (eligibility does not trigger F6 execution).
- [x] Eligibility requirements clearly separated from orchestration (F3 vs F4 vs F6).
- [x] No race conditions between LSN update and promotion query (immutable snapshots).
- [x] Decision record specifies deferral boundary clearly (this document).
- [x] Boundary aligns with existing F3 logic in `quorum.rs` (integration tested).
- [x] Doctrine compliance verified (durability, integrity, LSN contiguity, no split-brain).
- [x] No unsafe code, no gRPC, no unbounded SRPL semantics.

---

## Sign-Off

- **Designed by:** HA/DR and Backup Architect (F4 specialty skill).
- **Reviewed against:** F1 (DEC-019), F3 (DEC-020), D3, D4, V0 no-go rules.
- **Status:** ✅ Ready for F6+ implementation and integration.

---

## Appendix: Example Promotion Decision Flow

### Scenario: Primary Fails, Best Replica is Eligible

```rust
// F1: Compute LSN state
let primary_durable_lsn = Lsn::new(10000);
let replica1_safe_lsn = Lsn::new(10000);  // Caught up
let replica2_safe_lsn = Lsn::new(9500);   // Behind

// F3: Collect votes
let membership = HadrQuorumMembership::new(vec![node1, node2, node3])?;
let votes = vec![/* collected from quorum members */];
let promotion_request = HadrPromotionRequest::new(replica1_state, epoch+1, votes);

// F3: Evaluate promotion
let audit_record = evaluate_promotion(&promotion_request, &membership, &fencing);
// Returns: Approved { token, committed_safe_lsn, granted_voters }

// F4: Check eligibility (for audit/documentation)
let req1 = PromotionRequirements::new(replica1_safe_lsn, primary_durable_lsn, true, true);
let req2 = PromotionRequirements::new(replica2_safe_lsn, primary_durable_lsn, true, true);

let cand1 = PromotionCandidate::new(1, req1, current_epoch, 0);  // Best rank
let cand2 = PromotionCandidate::new(2, req2, current_epoch, 1);  // Worse

let best_cand = select_best_eligible_candidate(&[cand1, cand2])?;
// Returns: PromotionCandidate { replica_id: 1, rank: 0, ... }

// F6: Orchestration (operator/policy decides to promote)
// (If F3 returned Approved AND best candidate is eligible)
// F6 executes:
// 1. Acquire fencing token (epoch+1) as replica1.
// 2. Demote old primary.
// 3. Broadcast replica1 as new primary.
// 4. Emit audit event: promotion_success(replica_id=1, epoch=43, committed_lsn=10000).

// Result: Replica 1 is now the authoritative primary.
// All decisions were deterministic and can be replayed for audit.
```

---

## Related Documents

- **DEC-019:** F1 — WAL Shipping Runtime Model (segment protocol, LSN correlation, fencing).
- **DEC-020:** F3 — Quorum Runtime Sequencing (membership, voting, promotion consensus).
- **F1_SHIPPING_RUNTIME_DESIGN.md:** F1 implementation summary.
- **F3_QUORUM_RUNTIME_COMPLETION.md:** F3 implementation summary.
- **F5_BACKUP_EXECUTION_PLAN.md:** Backup and recovery implications (may reference promotion state).

---

**Document Version:** 1.0  
**Date:** 2026  
**Next Review:** After F6 implementation and integration with E4 (Executor).
