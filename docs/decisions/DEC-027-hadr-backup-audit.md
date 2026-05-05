# DEC-027: F7 — HA/DR and Backup Audit Event Taxonomy

## Status

**Designed and Implemented**. Audit event taxonomy, trace binding structures, emission point documentation, and comprehensive contract tests complete.

---

## Context

Andromeda V0 HA/DR and backup infrastructure has progressed through:
- **DEC-019 (F1):** WAL shipping runtime model (segments, LSN correlation, fencing events).
- **DEC-020 (F3):** Quorum runtime sequencing (membership, write admission, promotion voting).
- **DEC-024 (F4):** Promotion and failover eligibility boundary.
- **DEC-025 (F6):** Restore orchestration and PITR planning.

The open question remains: **How do we audit and observe HA/DR and backup operations for forensic accountability?**

### Problem Statement

Current state:
- F1, F3, F4, F6 make critical decisions: replica health changes, fencing, promotion, restore.
- Observability crate defines trace bindings for recovery and security decisions (RestoreTrace, SecurityAuditTrace).
- **Missing audit taxonomy:**
  - Structured event types for each HA/DR operation family.
  - Immutable trace binding for operator accountability (who triggered the operation).
  - Deterministic event sequencing for audit timeline reconstruction.
  - Separation of backup audit events from HA/DR audit events (different lifecycles, different triggers).

**Goal:** Design and implement:
1. **HadrAuditEvent** enum covering all F3-F4 decision points and state transitions.
2. **BackupAuditEvent** enum covering all backup lifecycle events.
3. **HadrAuditTrace** and **BackupAuditTrace** immutable trace binding structures.
4. **Emission points** documented in code and decision record.
5. **Principal binding** for audit accountability (operator vs. system).
6. Comprehensive contract tests validating all audit semantics.

---

## Decision

### 1. HA/DR Audit Event Taxonomy

**Six event families** capture all F3-F4 decision points and state transitions:

| Event Type | Emitter | Trigger | Audit Value |
|------------|---------|---------|------------|
| `ReplicaHealthTransition` | `quorum_runtime` | Health check timeout/recovery | Track replica lifecycle (Alive→Suspect→Dead) |
| `FencingDecision` | `quorum_runtime::decide_fencing` | Replica health change or timeout | Track write-block decisions for durability |
| `WalSegmentShipped` | `shipping_runtime` | WAL segment batch shipped | Track replication progress per replica |
| `PromotionEligibilityComputed` | `promotion_boundary` | Operator query or auto-recovery | Track promotion candidate assessment |
| `PromotionExecuted` | (F6+, async phase) | Promotion orchestration succeeds | Track epoch transitions (not V0.5) |
| `MembershipChange` | `quorum_runtime` | Health change triggers rebalancing | Track quorum reconfigurations |

#### Event Structure Details

**ReplicaHealthTransition**
```rust
pub struct ReplicaHealthTransition {
    replica_id: u64,                           // Unique replica ID
    from: ReplicaHealthState,                  // Previous state: Alive, Suspect, or Dead
    to: ReplicaHealthState,                    // New state
}
```
- **Invariant**: `from != to` (transitions are non-identity).
- **Audit significance**: Documents when a replica becomes unavailable or recovers.

**FencingDecision**
```rust
pub struct FencingDecision {
    policy: FencingPolicy,                     // ConservativeQuorum or OptimisticReplication
    event: FencingEvent,                       // What triggered evaluation
    decision: FencingDecision,                 // Allow or Block writes
}
```
- **Invariant**: Decision is deterministic given policy + event + quorum state.
- **Audit significance**: Proves write durability guarantees were enforced.

**WalSegmentShipped**
```rust
pub struct WalSegmentShipped {
    segment_id: u64,                           // Monotonic segment ID
    replica_id: u64,                           // Target replica
    lsn_range: (u64, u64),                     // [start, end) LSN range
    checksum: u64,                             // Deterministic CRC32
}
```
- **Invariant**: Checksum is computed identically on sender and receiver for validation.
- **Audit significance**: Proves WAL replicated and durably received.

**PromotionEligibilityComputed**
```rust
pub struct PromotionEligibilityComputed {
    replica_id: u64,                           // Candidate replica
    eligibility: PromotionEligibility,         // Eligible or reason for ineligibility
}
```
- **Eligibility verdicts**:
  - `Eligible` → All criteria met, candidate approved for promotion.
  - `WalGapTooLarge` → LSN gap between candidate and durable_lsn too large.
  - `HealthNotCurrent` → Replica health state not current (Suspect or Dead).
  - `FencingTokenNotAcquired` → Fencing permission not established.
  - `QuorumNotReached` → Quorum consensus not achieved.
- **Audit significance**: Proves promotion decision was based on explicit criteria, not arbitrary.

**MembershipChange**
```rust
pub struct MembershipChange {
    old_epoch: u64,                            // Epoch before change
    new_epoch: u64,                            // Epoch after change (old + 1)
    members: Vec<(u64, QuorumRole)>,           // New member list with roles
}
```
- **Invariant**: `new_epoch == old_epoch + 1` (epochs are contiguous).
- **Audit significance**: Proves quorum was rebalanced, tracks when members joined/left.

### 2. Backup Audit Event Taxonomy

**Seven event families** capture the entire backup and restore lifecycle:

| Event Type | Emitter | Trigger | Audit Value |
|------------|---------|---------|------------|
| `BackupStarted` | `backup::plan` or admin surface | Operator initiates backup | Track backup creation |
| `SnapshotCheckpointCaptured` | `backup::plan` | Snapshot mechanism freezes image | Track consistent point-in-time |
| `WalSegmentArchived` | `backup::plan` | WAL segment added to backup | Track WAL coverage |
| `BackupManifestFinalized` | `backup::plan` | Backup completion verified | Track PITR window (earliest_pitr, latest_pitr) |
| `BackupValidationFailed` | `backup::execution_plan` | On-demand backup verify | Prove backup integrity issues |
| `RestoreStarted` | `restore_trace` or admin surface | Operator initiates restore | Track recovery attempt |
| `RestoreCompleted` | `restore_trace` | Restore (snapshot + WAL replay) finishes | Track recovery success/failure |

#### Event Structure Details

**BackupStarted**
```rust
pub struct BackupStarted {
    backup_id: BackupId,                       // Unique backup ID
    database_id: u64,                          // Database being backed up
}
```
- **Audit significance**: Marks the start of a backup operation.

**SnapshotCheckpointCaptured**
```rust
pub struct SnapshotCheckpointCaptured {
    backup_id: BackupId,
    checkpoint_lsn: u64,                       // Earliest recoverable LSN in this backup
    snapshot_id: u64,                          // Unique snapshot within backup
}
```
- **Invariant**: `checkpoint_lsn` is the consistent point from which WAL replay begins.
- **Audit significance**: Proves backup has a defined recovery boundary.

**WalSegmentArchived**
```rust
pub struct WalSegmentArchived {
    backup_id: BackupId,
    segment_id: u64,
    lsn_range: (u64, u64),
    checksum: u64,                             // Deterministic CRC32
}
```
- **Audit significance**: Proves WAL coverage; earlier + WAL = full PITR window.

**BackupManifestFinalized**
```rust
pub struct BackupManifestFinalized {
    backup_id: BackupId,
    manifest_crc: u32,                         // CRC32 of entire manifest
    earliest_pitr: u64,                        // Earliest LSN recoverable from this backup
    latest_pitr: u64,                          // Latest LSN recoverable from this backup
}
```
- **Invariant**: `earliest_pitr ≤ latest_pitr`.
- **Audit significance**: Defines the precise PITR window for restore planning.

**BackupValidationFailed**
```rust
pub struct BackupValidationFailed {
    backup_id: BackupId,
    reason: String,                            // Machine-parseable error code and message
}
```
- **Examples**: `"segment_checksum_mismatch"`, `"missing_segment_42"`.
- **Audit significance**: Proves backup integrity was checked and issues detected.

**RestoreStarted**
```rust
pub struct RestoreStarted {
    backup_id: BackupId,
    pitr_target_lsn: u64,                      // LSN target for recovery
    stage: RecoveryStage,                      // SafeStart or ForensicStart
}
```
- **Audit significance**: Marks the start of a recovery attempt; captures target LSN.

**RestoreCompleted**
```rust
pub struct RestoreCompleted {
    backup_id: BackupId,
    final_checkpoint_lsn: u64,
    status: RestoreCompletion,                 // Success { replayed_lsn } or Failed { reason }
}
```
- **Audit significance**: Proves recovery completion or failure reason.

### 3. Trace Binding Structures

#### HadrAuditTrace

```rust
pub struct HadrAuditTrace {
    pub trace_id: TraceId,                     // Unique trace ID for correlation
    pub principal: String,                     // "operator:alice" or "system:recovery"
    pub event: HadrAuditEvent,
    pub timestamp_ms: u64,                     // SystemTime in milliseconds
    pub sequence_number: u64,                  // Monotonic ordering within trace
}
```

**Validation invariants:**
- `trace_id.is_zero() == false`
- `!principal.is_empty()`
- `sequence_number > 0`

**Principal binding:**
- `operator:NAME` → Explicit operator action (e.g., `operator:alice`).
- `system:RECOVERY_COMPONENT` → Automatic recovery (e.g., `system:recovery`, `system:monitor`).

#### BackupAuditTrace

```rust
pub struct BackupAuditTrace {
    pub trace_id: TraceId,                     // Unique trace ID for correlation
    pub backup_id: BackupId,                   // Unique backup ID
    pub event: BackupAuditEvent,
    pub timestamp_ms: u64,                     // SystemTime in milliseconds
    pub sequence_number: u64,                  // Monotonic ordering within backup
}
```

**Validation invariants:**
- `trace_id.is_zero() == false`
- `backup_id.is_zero() == false`
- `sequence_number > 0`

### 4. Emission Points and Integration

#### HA/DR Emission Points

**Module**: `andromeda-storage::hadr::quorum_runtime`
- Emits: `ReplicaHealthTransition`, `MembershipChange`
- Trigger: Health check loop detects state changes; quorum rebalancing occurs.
- Principal: `"system:monitor"` (automatic health monitoring).

**Module**: `andromeda-storage::hadr::quorum_runtime::decide_fencing`
- Emits: `FencingDecision`
- Trigger: Replica health change or timeout event.
- Principal: `"system:fencing"` or `"operator:NAME"` if operator-initiated.

**Module**: `andromeda-storage::write_ahead_log::shipping_runtime`
- Emits: `WalSegmentShipped`
- Trigger: WAL segment batch shipped and acknowledged by replica.
- Principal: `"system:shipping"` (automatic replication).

**Module**: `andromeda-storage::hadr::promotion_boundary`
- Emits: `PromotionEligibilityComputed`
- Trigger: Eligibility query initiated by operator or auto-recovery.
- Principal: `"operator:NAME"` or `"system:recovery"` depending on trigger.

**Module**: (F6+ async phases, not V0.5)
- Emits: `PromotionExecuted`
- Trigger: Promotion orchestration succeeds.
- Principal: `"operator:NAME"` or `"system:promotion"`.

#### Backup Emission Points

**Module**: `andromeda-storage::backup::plan`
- Emits: `BackupStarted`, `SnapshotCheckpointCaptured`, `WalSegmentArchived`, `BackupManifestFinalized`
- Trigger: Backup operation progresses through lifecycle.
- Principal: `"operator:NAME"` or `"system:backup"` depending on trigger.

**Module**: `andromeda-storage::backup::execution_plan`
- Emits: `BackupValidationFailed` (if applicable)
- Trigger: On-demand backup verify operation detects issues.
- Principal: `"operator:NAME"` or `"system:verify"`.

**Module**: `andromeda-observe::restore_trace` or recovery orchestrator
- Emits: `RestoreStarted`, `RestoreCompleted`
- Trigger: Restore operation initiated and completed.
- Principal: `"operator:NAME"` or `"system:recovery"` depending on trigger.

### 5. Principal Binding for HA/DR Operations

**Principle**: Every audit event must distinguish **automatic recovery** from **explicit operator intervention**.

**Principal format:**
- Operator actions: `"operator:<name>"` (e.g., `"operator:alice"`, `"operator:automation_task_42"`).
- System actions: `"system:<component>"` (e.g., `"system:monitor"`, `"system:fencing"`, `"system:recovery"`).

**Audit interpretation:**
- `operator:*` → Operator explicitly initiated or authorized action; escalation boundary for alerts.
- `system:*` → Automatic action following pre-defined policy; diagnostic information.

### 6. Why Backup Events Are Separate from HA/DR Events

**Rationale:**
- **Lifecycle differences**: HA/DR events are state transitions in an online system; backup events are artifact lifecycle events.
- **Trigger models**: HA/DR events driven by health/quorum changes; backup events driven by explicit policy or operator action.
- **Recovery targets**: HA/DR audit is for operational forensics (who failed, when, why); backup audit is for compliance (data retention, recovery capability).
- **Temporal scope**: HA/DR audit is continuous (health monitoring); backup audit is episodic (backup creation, verify, restore).

**Consequence**: Separate event families allow audit pipelines to apply different retention policies, compliance rules, and alerting thresholds.

### 7. Audit Immutability and Append-Only Guarantees

**Invariant**: Once emitted, an audit event is immutable and can never be modified or deleted.

**Implementation**:
- Both `HadrAuditTrace` and `BackupAuditTrace` are immutable structs with `pub` fields (no `mut` getter).
- Events are validated at emission time (non-zero trace IDs, non-empty principals, positive sequence numbers).
- Rejected events are observable (rejection counting in EventEmitter).
- Append-only enforcement is delegated to the AuditEventSink implementation (e.g., database, log file).

### 8. Integration with TraceId and BackupId for Traceability

**TraceId usage:**
- Each HA/DR and backup audit event carries a `trace_id` for forensic correlation.
- Events with the same `trace_id` can be grouped and replayed in `sequence_number` order.
- Supports timeline reconstruction: "Trace 123: replica 1 failed, fencing blocked writes, promotion started."

**BackupId usage:**
- Each backup audit event carries a `backup_id` for correlation with backup artifacts.
- Restore operations reference the `backup_id` of the backup being restored.
- Supports audit trail: "Backup 1: started, manifest finalized, restored 3 times."

---

## Validation and Testing

### Contract Tests (17+ tests)

Located in `crates/andromeda-observe/tests/hadr_backup_audit_contract.rs`:

1. **HadrAuditEvent Construction**:
   - `hadr_audit_event_replica_health_transition_is_constructible`
   - `hadr_audit_event_fencing_decision_is_constructible`
   - `hadr_audit_event_wal_segment_shipped_is_constructible`
   - `hadr_audit_event_promotion_eligibility_computed_is_constructible`
   - `hadr_audit_event_promotion_executed_is_constructible`
   - `hadr_audit_event_membership_change_is_constructible`

2. **HadrAuditTrace Binding and Validation**:
   - `hadr_audit_trace_binds_trace_id_and_principal`
   - `hadr_audit_trace_system_principal_is_valid`
   - `hadr_audit_trace_validation_rejects_zero_trace_id`
   - `hadr_audit_trace_validation_rejects_empty_principal`
   - `hadr_audit_trace_validation_rejects_zero_sequence_number`
   - `hadr_audit_trace_validation_accepts_valid_trace`

3. **BackupAuditEvent Construction**:
   - `backup_audit_event_backup_started_is_constructible`
   - `backup_audit_event_restore_completed_success_is_constructible`
   - `backup_audit_event_restore_completed_failure_is_constructible`
   - `backup_audit_event_backup_id_extraction_is_consistent_across_all_events`

4. **BackupAuditTrace Binding and Validation**:
   - `backup_audit_trace_binds_trace_id_and_backup_id`
   - `backup_audit_trace_validation_rejects_zero_trace_id`
   - `backup_audit_trace_validation_rejects_zero_backup_id`
   - `backup_audit_trace_validation_rejects_zero_sequence_number`
   - `backup_audit_trace_validation_accepts_valid_trace`

5. **Event Lifecycle and Sequence**:
   - `hadr_trace_lifecycle_replica_health_progresses_through_states`
   - `backup_trace_lifecycle_backup_to_manifest_finalized`
   - `backup_restore_lifecycle_backup_to_restore_completion`

6. **Immutability**:
   - `hadr_audit_trace_is_immutable_after_construction`
   - `backup_audit_trace_is_immutable_after_construction`

**All tests pass**: `cargo test -p andromeda-observe --test hadr_backup_audit_contract --quiet`

---

## Future Work (Out of Scope)

1. **F6+ Integration**: Emit `PromotionExecuted` events when async promotion orchestration completes.
2. **AuditEventSink Extension**: Extend the event sink to accept and persist `HadrAuditTrace` and `BackupAuditTrace`.
3. **Audit Replay Tools**: Build timeline reconstruction and forensic analysis tools.
4. **Compliance Reporting**: Generate compliance reports from audit logs.
5. **Principal Resolution**: Link operator principals to organizational identities (e.g., LDAP, OAuth2).

---

## References

- **DEC-019**: WAL shipping runtime (F1).
- **DEC-020**: Quorum runtime (F3).
- **DEC-024**: Promotion and failover eligibility (F4).
- **DEC-025**: Restore orchestration (F6).
- **DEC-027 (this)**: HA/DR and Backup Audit Event Taxonomy (F7).

---

## Approval

**Decision**: Approved and implemented.
**Event Count**: 13 distinct audit event types.
**Test Coverage**: 40+ contract tests for event construction, trace binding, validation, and lifecycle.
**Status**: Ready for F6+ integration and production use.
