# DEC-025: F6 — Restore and PITR Execution Plan

## Status

**Designed and Implemented**. `RestoreOrchestration` type, pure planning functions, audit trace binding, comprehensive test suite (15+ tests), and this decision record complete.

---

## Context

Andromeda V0 HA/DR foundation has progressed through:
- **DEC-019 (F1):** WAL shipping runtime model (segments, LSN correlation, fencing events).
- **DEC-020 (F3):** Quorum runtime sequencing (membership, write admission, promotion voting).
- **DEC-024 (F4):** Promotion and failover eligibility boundary (no automatic failover).
- **F5 (Backup):** Physical backup execution plan (manifest validation, WAL archive copy sequencing).

The open question remains: **How do we restore from backup to a point-in-time (PITR) with durable, auditable semantics?**

### Problem Statement

Current state:
- F5 produces a `BackupExecutionPlan` with manifest validation, extent copy tasks, and WAL segment copy sequencing.
- F1 shipped WAL archive to backup storage alongside cold snapshot.
- F4 prevents automatic failover; application must decide when to promote a replica.

**Missing orchestration:**
- **What are the restore inputs?** Backup manifest, PITR target LSN, recovery stage, validation policy.
- **How do we validate restore prerequisites?** Manifest structure, PITR LSN in range, WAL archive contiguity.
- **How do we plan the replay sequence?** Identify which WAL segments to replay to reach PITR target LSN.
- **How do we track the restore?** Immutable audit trace binding trace ID, backup ID, PITR target, recovery stage, and final status.
- **When are decisions deferred to async phases?** Restore execution (I/O + replay) is not in the critical path.

**Goal:** Separate restore planning from execution by defining:
1. What F5 computes (backup manifest validation and copy sequencing).
2. What F6 designs (restore orchestration decisions and audit binding).
3. What async phases execute (WAL replay and checkpoint reconstruction).
4. Ensure restore is always explicit (no "latest" default PITR target).

---

## Decision

### 1. Restore Orchestration Types

#### RestoreOrchestration (Decision Type)

```rust
pub struct RestoreOrchestration {
    pub backup_manifest: BackupManifest,  // From F5
    pub pitr_target_lsn: Lsn,             // App-selected, not automatic
    pub recovery_stage: RecoveryStage,    // SafeStart or ForensicStart
    pub validation_policy: RestoreValidationPolicy,  // Full or Minimal
    pub audit: RestoreAuditTrace,         // Immutable trace binding
}
```

**Invariants:**
- Manifest is valid (CRC, identity fields, WAL bounds).
- PITR target LSN is within `[manifest.wal_archive.start, manifest.wal_archive.end_inclusive]`.
- Audit trace has non-zero trace ID and backup ID.
- Recovery stage is explicit (no automatic inference).
- Validation policy matches operator intent (no silent downgrades).

**Rationale:**
- `backup_manifest`: All restore inputs derive from the durable backup artifact, not hot memory.
- `pitr_target_lsn`: Application-selected, never automatic. Defers operational policy to caller.
- `recovery_stage`: Supports both standard (`SafeStart`) and forensic (`ForensicStart`) modes.
- `validation_policy`: Allows operators to trade validation cost for speed; Full is safer, Minimal is operator-responsible.
- `audit`: Every restore is traced for forensics and compliance.

#### RestoreValidationPolicy

```rust
pub enum RestoreValidationPolicy {
    /// Verify manifest signature and WAL checksums during replay
    Full,

    /// Skip expensive verification (operator responsibility).
    /// Only checks LSN contiguity, not byte-level integrity.
    Minimal,
}
```

**Rationale:**
- **Full**: Default for untrusted backups or compliance-sensitive restores. Validates manifest signature, WAL record checksums, and byte-level integrity.
- **Minimal**: For operators who trust the backup and need fast restore. Validates only LSN contiguity and segment ordering; operator assumes no corruption.
- Neither policy skips mandatory checks (manifest identity, PITR LSN range, WAL chain contiguity).

#### RecoveryStage

```rust
pub enum RecoveryStage {
    /// Standard startup: assume no corruption, replay from checkpoint LSN
    SafeStart,

    /// Forensic startup: detect and report corruption; do not repair in-place
    ForensicStart,
}
```

**Rationale:**
- **SafeStart**: Normal case. Replay from manifest checkpoint LSN; assume snapshot is healthy.
- **ForensicStart**: Post-incident mode. Collect corruption signals (LSN gaps, checksum failures) in audit trail; don't attempt repair (F7+ responsibility).

#### RestoreAuditTrace

```rust
pub struct RestoreAuditTrace {
    pub trace_id: TraceId,                    // Unique restore attempt ID
    pub backup_id: BackupId,                  // Which backup was restored
    pub pitr_target_lsn: Lsn,                 // Target LSN for this restore
    pub stage: RecoveryStage,                 // SafeStart or ForensicStart
    pub checksum: u64,                        // Deterministic hash of manifest + WAL bounds
    pub completion_status: Option<RestoreCompletion>,  // Final status (None until complete)
}

pub enum RestoreCompletion {
    Success {
        replayed_lsn: Lsn,           // Final LSN reached after replay
        final_checkpoint_lsn: Lsn,   // Checkpoint LSN at restore end
    },
    Failed { reason: String },       // Audit-trail reason for failure
}
```

**Invariants:**
- `trace_id` is non-zero (must be unique per restore attempt).
- `backup_id` is non-zero (must identify a valid backup).
- `checksum` is deterministic (same manifest → same checksum).
- `completion_status` remains `None` until restore execution completes.
- If restore succeeds, `replayed_lsn ≥ pitr_target_lsn` (or equals target if final segment contains target).

**Rationale:**
- Immutable trace captures restore inputs and final disposition for forensics.
- Deterministic checksum enables audit verification and replay proof.
- `completion_status` binding is deferred to async phases (not in critical path).

### 2. Restore Pipeline Functions (Pure)

All functions are **pure** (no I/O, no async) and operate on durable backup artifacts.

#### validate_restore_prerequisites()

```rust
pub fn validate_restore_prerequisites(
    manifest: &BackupManifest,
    pitr_lsn: Lsn,
) -> AndromedaResult<()>
```

**Checks:**
- Manifest is valid (identity fields, CRC, WAL bounds).
- WAL archive range is valid (start ≤ end_inclusive).
- PITR LSN is within archive range: `pitr_lsn ∈ [start, end_inclusive]`.

**Returns:**
- `Ok(())` if all checks pass.
- `Err` with Storage kind if any check fails.

**Rationale:**
- Gate-keeping function; must succeed before planning replay.
- Operates only on immutable, durable inputs.

#### plan_replay_segments()

```rust
pub fn plan_replay_segments(
    manifest: &BackupManifest,
    pitr_lsn: Lsn,
    segments: &[WalSegmentDescriptor],
) -> AndromedaResult<Vec<WalSegmentToReplay>>
```

**Validates:**
- All segments are individually valid (format version, record count, LSN bounds).
- Segments form a **contiguous LSN chain** without gaps:
  - First segment: `base_previous_lsn = None`.
  - Chaining: `segment[i].last_lsn.try_next() == segment[i+1].first_lsn`.
  - All segments within archive range.

**Output:**
- Ordered list of `WalSegmentToReplay` (metadata only, no actual data).
- List includes all segments up to and including the segment containing PITR LSN.
- Each segment marked with `contains_pitr_target` flag.

**Returns:**
- `Ok(segments_to_replay)` if plan is valid.
- `Err` if chain is broken, PITR not found, or any segment is invalid.

**Rationale:**
- Deterministic planning: same inputs → same output.
- No I/O; operates only on WAL segment descriptors.
- Stops after PITR segment (don't plan segments not needed for PITR).
- Validates contiguity (no silent gaps).

#### compute_restore_checksum()

```rust
pub fn compute_restore_checksum(manifest: &BackupManifest) -> u64
```

**Computation:**
- Deterministic hash combining:
  - Backup ID
  - Snapshot ID
  - Manifest version
  - Checkpoint LSN
  - WAL archive bounds (start, end_inclusive)
  - WAL record count

**Rationale:**
- Stable hash across invocations if manifest and archive unchanged.
- Enables audit trail verification (same manifest → same checksum → proof of unchanged backup).
- Used for audit binding in `RestoreAuditTrace`.

### 3. Integration with F4 Promotion Boundary

**Restore and promotion are independent concerns:**

- **Promotion (F4):** Eligibility of replica to become primary.
  - Checked via `PromotionRequirements` (replica LSN ≥ primary LSN, quorum member, connected).
  - Does NOT depend on backup restore.

- **Restore (F6):** Recovery of database from backup to PITR.
  - Checked via `validate_restore_prerequisites()` and `plan_replay_segments()`.
  - May follow demotion of primary or loss of all replicas.

**Interaction scenarios:**

1. **Primary demotion after promotion:** If promoted replica becomes primary and later crashes, new primary can restore from its own backup or from a peer's backup.
2. **Total cluster loss:** All nodes down; first node to recover restores from backup + WAL archive. Recovery stage can be `ForensicStart` to detect corruption.
3. **Backup restore during failover window:** Operator can restore a backup snapshot while deciding which replica to promote (F4) and when (F6+ orchestration).

**Invariant:**
- A promoted primary's backup epoch must match or predate the promotion epoch (ensures backup was captured before promotion).

### 4. Audit Trail Requirements

**Each restore attempt MUST:**

1. **Create a unique RestoreAuditTrace** at the start of orchestration (with trace_id).
2. **Bind checkpoint information** before replay begins (PITR target, recovery stage, validation policy).
3. **Compute deterministic checksum** of manifest + WAL archive (for audit verification).
4. **Bind completion status** after replay finishes (success LSN or failure reason).

**Audit properties:**

- **Trace binding:** Same trace_id used throughout restore attempt for forensic correlation.
- **Immutability:** Trace and checksum never change after creation.
- **Non-repudiation:** Completion status records final disposition (success with LSN or failure reason).

**Why deferred to async phases:**
- Restore orchestration (F6) is in critical path; it must return quickly.
- WAL replay and checkpoint reconstruction (async) write the completion status.
- Observer can track in-progress restores by polling for `completion_status` presence.

### 5. Failure Modes and Recovery

**Validation failures (return error in F6):**
- PITR LSN out of range → Cannot plan restore, operator must select valid LSN.
- Manifest CRC invalid → Backup is corrupted, cannot be trusted.
- WAL chain gap detected → Backup is incomplete, cannot reach PITR LSN reliably.
- Zero trace ID or backup ID → Audit trace is invalid, restore cannot proceed.

**Replay failures (captured in RestoreCompletion::Failed):**
- WAL record checksum mismatch (Full validation policy) → Stop replay, report failure.
- Logical conflict detected (ForensicStart mode) → Record corruption boundary, continue forensic scan.
- Storage I/O error → Stop replay, report failure reason.

**Recovery options:**
- Retry with `Minimal` validation policy (skip checksums, trust manifest).
- Retry with different backup artifact (if multiple backups available).
- Enter forensic analysis mode to understand corruption signals.

---

## Rationale

### Why Explicit PITR Target?

- **No "latest" default:** Prevents accidental full-database restore when partial recovery was intended.
- **Application-driven policy:** Business logic determines acceptable recovery point (RPO).
- **Audit trail:** Every restore captures the explicit PITR selection (non-repudiation).

### Why Deterministic Checksum?

- **Audit verification:** Observer can recompute checksum of same manifest; match proves manifest unchanged during restore.
- **Replay proof:** Checksum in audit trail proves which backup was used.
- **Forensic correlation:** Checksum links backup metadata to restore attempt for incident analysis.

### Why Separate Planning from Execution?

- **Critical path:** F6 must return quickly (decision only, no I/O).
- **Async execution:** WAL replay and checkpoint reconstruction happen after F6 returns.
- **Observable progress:** Trace binding allows async phases to record progress and final status.

### Why Validation Policies?

- **Full (safer):** For untrusted backups or compliance-sensitive restores.
- **Minimal (faster):** For operators who trust backup and need fast recovery.
- **No silent downgrade:** Policy is explicit in orchestration; no automatic fallback.

---

## Scope and Boundaries

### F6 (This Module) Owns
- RestoreOrchestration type and validation.
- Pure planning functions (no I/O).
- Audit trace binding inputs (checksum, trace ID, backup ID).

### F5 Owns
- Backup manifest validation and copy sequencing.
- Extent and WAL segment copy tasks.
- Backup resource limits and parallelization.

### Async Phases Own
- WAL replay from segments.
- Checkpoint reconstruction.
- Corruption detection and repair (ForensicStart forensics).
- Final audit trace completion binding.

### F4 Owns
- Promotion eligibility computation.
- Failover candidate evaluation.
- No interaction with F6 restore (independent concerns).

---

## Tests and Verification

**Contract test suite (`tests/restore_contract.rs`) covers:**

1. **PITR LSN Validation** (5 tests)
   - Within range (start, middle, end)
   - Out of range (below, above)

2. **WAL Segment Replay Planning** (7 tests)
   - Single segment containing PITR
   - Multiple segments with correct chaining
   - Stops after PITR segment
   - Empty segment list rejection
   - LSN gap detection
   - First segment validation (no base_previous_lsn)

3. **Restore Checksum** (3 tests)
   - Deterministic (same manifest → same checksum)
   - Varies with backup ID
   - Varies with WAL range

4. **Audit Trace Validation** (6 tests)
   - Valid inputs acceptance
   - Zero trace ID rejection
   - Zero backup ID rejection
   - Completion binding (success and failure)

5. **RestoreOrchestration** (6 tests)
   - Construction succeeds
   - Manifest validation
   - PITR out-of-range rejection
   - ForensicStart support
   - Minimal validation policy support

6. **PITR Checkpoint Reconstruction** (2 tests)
   - Single segment preserves bounds
   - Multiple segments preserve chain (LSN continuity)

**Total: 15+ contract tests**

**Compile verification:**
```bash
cargo check -p andromeda-storage --quiet
```

**Test execution:**
```bash
cargo test -p andromeda-storage --test restore_contract --quiet
```

---

## Decision Records Referenced

- **DEC-019 (F1):** WAL shipping runtime (segments, LSN correlation, fencing).
- **DEC-020 (F3):** Quorum runtime sequencing (membership, promotion voting).
- **DEC-024 (F4):** Promotion and failover eligibility boundary.

---

## Approval and Sign-Off

**Decision Owner:** HA/DR and Backup Architect

**Reviewed By:** Storage and Observability teams

**Effective Date:** [Implementation completion date]

**Related Artifacts:**
- `crates/andromeda-storage/src/restore.rs` — RestoreOrchestration type and planning functions
- `crates/andromeda-observe/src/restore_trace.rs` — RestoreAuditTrace and RestoreCompletion types
- `crates/andromeda-storage/tests/restore_contract.rs` — Contract test suite
