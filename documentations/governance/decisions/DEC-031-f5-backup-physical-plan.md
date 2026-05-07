# DEC-031: F5 Backup Physical Plan — Design Decisions and Invariants

**Status:** ACCEPTED  
**Date:** Q1 2026  
**Task:** F5 — Plan Physical Backup Execution  
**Author:** HA/DR and Backup Architect  
**Stakeholders:** F1 (WAL Shipping), F4 (WAL Durability), F6 (Restore), F7 (Audit Traces), Recovery Team

---

## Executive Summary

This decision record documents the design of **BackupPhysicalPlan**, which orchestrates deterministic, crash-safe, I/O-budgeted backup execution from HotStore (NVMe) and ColdStore (HDD) for Andromeda V0.5 recoverable vertical slice.

### Key Principles

1. **Planning is pure** — no side effects, no I/O during plan creation
2. **Determinism** — same segment layout produces identical plans
3. **LSN-binding** — plans locked to catalog_snapshot_lsn; cannot replay across different snapshots
4. **Crash-safety** — checkpoints enable resumption without re-scanning
5. **I/O-budgeting** — respects NVMe (1000 MB/s) and HDD (100 MB/s) throughput ceilings

---

## Context

### Problem Statement

Andromeda requires a physical backup capability to copy cold snapshots and WAL archives to external storage. The backup must:

- Deterministically plan I/O tasks from segment layouts
- Respect storage layer throughput constraints
- Enable resumption after crashes without losing progress
- Validate WAL archive coverage and compute PITR windows

### Related Tasks

- **F1** (WAL Shipping): Produces WAL segments for archive
- **F4** (WAL Durability): Ensures WAL visibility before backup commits
- **F6** (Restore): Uses BackupManifest to validate PITR ranges
- **F7** (Audit Traces): Records BackupManifestFinalizedEvent

### Upstream Dependencies

- **B5** (Cold Snapshot Stability): Segment state tracking, immutability guarantees
- **B6** (Recovery Integrity): LSN semantics, checkpoint safety
- **F2** (HADR Stream Mapping): Quorum agreements, recovery boundaries

---

## Design Decisions

### DEC-031-1: Physical Plan is Pure (No I/O)

**Decision:** BackupPhysicalPlan is created in-memory without side effects. All I/O operations are deferred to scheduler and execution layers.

**Rationale:**

- Testability: Plans can be created and validated in unit tests without touching disk
- Determinism: Same inputs always produce identical plans
- Safety: If planning fails, no partial I/O has occurred
- Clarity: Plan creation is a pure computation; execution is a separate concern

**Invariant:**
```rust
impl BackupPhysicalPlan {
    pub fn new(...) -> Self { /* no I/O */ }
    pub fn validate(&self) -> Result<()> { /* no I/O */ }
}
```

**Verification:**
- ✓ No `File::open()`, `std::fs`, or I/O syscalls in `physical_plan.rs`
- ✓ All tests pass without filesystem mocking
- ✓ Plan can be serialized/deserialized for checkpoints

---

### DEC-031-2: Checkpoint Safety — Metadata Durability

**Decision:** Backup checkpoints are persisted to metadata files and fsync'd after each write. Checksums protect data integrity on resumption.

**Rationale:**

- Crash Resilience: If backup crashes mid-operation, last durable checkpoint indicates progress
- Resume Guarantee: Can skip already-scanned pages and resume from next page
- Integrity: CRC-32 checksum validates completed portion hasn't been corrupted
- Simplicity: No complex state machines; checkpoint is atomic unit of progress

**Invariants:**

1. **Checkpoint Durability**: Every checkpoint is fsync'd before returning control to caller
2. **No Re-scanning**: On crash, resume from `last_completed_page`, not from beginning
3. **Checksum Validation**: Recompute checksum of completed portion; must match checkpoint
4. **Atomic Updates**: Metadata file is updated atomically (write + fsync)

**Contract:**
```rust
pub fn persist_backup_checkpoint(&self, checkpoint: &BackupCheckpoint) -> Result<()> {
    // 1. Write checkpoint to metadata file
    // 2. fsync() to ensure durability
    // 3. Return only after durability confirmed
}

pub fn recover_backup_from_checkpoint(&self, backup_id: BackupId) -> Result<BackupRecoveryInfo> {
    // Load last checkpoint
    // Skip already-scanned pages
    // Return resume point
}
```

**Verification:**
- ✓ Checkpoint struct includes epoch, phase, checksum
- ✓ Checkpoint validation ensures non-zero fields
- ✓ Tests verify checksum mismatch detection

---

### DEC-031-3: I/O Budgeting — Storage Tier Throughput Ceilings

**Decision:** BackupIOScheduler respects separate throughput budgets for NVMe (1000 MB/s) and HDD (100 MB/s).

**Rationale:**

- Storage Constraints: NVMe and HDD have different I/O characteristics
- Oversubscription Prevention: Prevents starving other workloads
- Observable Behavior: Peak throughput is max(nvme_budget, hdd_budget), not sum
- Throttling Support: Tasks can be paused/resumed without data loss

**Invariants:**

1. **NVMe Priority**: HotStore (NVMe) extents are scheduled first (priority 0)
2. **HDD Sequential**: ColdStore (HDD) extents are scheduled second (priority 1+)
3. **WAL Sequential**: WAL archive is finalized last (after all pages)
4. **Budget Respect**: Never emit tasks that would exceed budget when run sequentially

**Budget Formula:**
```
peak_throughput_mbps = max(nvme_throughput_mbps, hdd_throughput_mbps)
total_duration_ms = sum(task.estimated_io_ms for each task)
```

**Verification:**
- ✓ Scheduler creates HotStore tasks before ColdStore tasks
- ✓ Schedule.respects_budget() validates against budgets
- ✓ Tests verify budget enforcement

---

### DEC-031-4: LSN Binding — Catalog Snapshot Immutability

**Decision:** BackupPhysicalPlan is bound to a specific `catalog_snapshot_lsn`. The plan cannot be replayed with a different LSN.

**Rationale:**

- Safety: Prevents accidental mixing of plans from different snapshots
- Clarity: Each backup is tied to one specific point-in-time
- Recovery Contract: F6 (Restore) validates PITR targets match the snapshot's LSN
- Audit Trail: Trace IDs and epochs tie plans to specific backup requests

**Invariant:**
```rust
pub struct BackupPhysicalPlan {
    pub catalog_snapshot_lsn: Lsn,  // Immutable; plan locked to this LSN
    pub trace_id: TraceId,           // Audit trail reference
    pub created_epoch: u64,          // Timestamp of plan creation
}

impl BackupPhysicalPlan {
    pub fn validate(&self) -> Result<()> {
        if self.catalog_snapshot_lsn.is_zero() {
            return Err("LSN must be non-zero");
        }
        // ... LSN binding prevents plan reuse across snapshots
    }
}
```

**Verification:**
- ✓ BackupPhysicalPlan.validate() rejects zero LSN
- ✓ BackupManifest includes snapshot boundary with matching LSN
- ✓ WAL archive validation ensures coverage of catalog_snapshot_lsn

---

### DEC-031-5: WAL Archive Integration — PITR Window Computation

**Decision:** WAL archive must be contiguous and cover the catalog snapshot LSN. PITR window is `[catalog_snapshot_lsn, shipped_wal_lsn]`.

**Rationale:**

- Restore Contract: F6 needs to know earliest and latest restorable points
- Completeness: WAL must include all changes from snapshot to end
- Clarity: PITR window is computed from manifest, immutable after finalization
- Safety: LSN gaps or insufficient coverage cause backup finalization to fail

**Invariant:**
```
shipped_wal_lsn >= catalog_snapshot_lsn  (WAL must cover snapshot)

PITR_window = [
    earliest = catalog_snapshot_lsn,
    latest = shipped_wal_lsn
]
```

**Contract:**
```rust
pub fn finalize_backup_with_wal_archive(
    plan: &BackupPhysicalPlan,
    manifest_template: &BackupManifest,
    shipped_wal_lsn: Lsn,
    wal_segment_count: u64,
) -> Result<(BackupManifest, BackupManifestFinalizedEvent)> {
    // Validate WAL archive covers catalog snapshot
    // Create finalized manifest with correct WAL range
    // Emit audit event for F7 traces
}
```

**Verification:**
- ✓ WAL archive validation rejects shipped_wal_lsn < catalog_snapshot_lsn
- ✓ PITR window tests verify boundary inclusion
- ✓ BackupManifestFinalizedEvent emitted on success

---

## Architectural Diagram

```
BackupPhysicalPlan (Pure, deterministic planning)
    │
    ├─► validate()  [No I/O]
    │
    ├─► Segments:
    │   ├─ HotStoreScan (priority 0)
    │   └─ ColdStoreScan (priority 1+)
    │
    └─► LSN-bound to catalog_snapshot_lsn

         │
         ▼
BackupIOScheduler (Deterministic I/O task scheduling)
    │
    ├─► schedule_page_scan()
    │   ├─ Create tasks from segments
    │   ├─ Respect NVMe/HDD budgets
    │   └─ Return BackupIOSchedule
    │
    └─► No I/O; returns plan only

         │
         ▼
BackupCheckpointManager (Crash-safe resumption)
    │
    ├─► persist_backup_checkpoint()
    │   ├─ Write to metadata file
    │   ├─ fsync() for durability
    │   └─ Validate integrity
    │
    ├─► recover_backup_from_checkpoint()
    │   ├─ Load last checkpoint
    │   ├─ Return resume info
    │   └─ Skip already-scanned pages
    │
    └─► validate_checksum_after_resumption()
        └─ Detect corruption on resume

         │
         ▼
WalArchiveIntegration (WAL coverage validation + manifest finalization)
    │
    ├─► validate_wal_archive()
    │   └─ Ensure WAL covers catalog snapshot
    │
    ├─► finalize_backup_with_wal_archive()
    │   ├─ Create BackupManifest
    │   ├─ Compute PITR window
    │   └─ Emit BackupManifestFinalizedEvent (F7)
    │
    └─► validate_pitr_target()
        └─ Reject targets outside PITR window (used by F6)
```

---

## Relationship to Other Modules

### F1 (WAL Shipping)
- **Input**: WAL segments from F1's shipped records
- **Output**: shipped_wal_lsn passed to finalize_backup_with_wal_archive()
- **Contract**: WAL must be contiguous; gaps cause backup to fail

### F4 (WAL Durability)
- **Input**: Visibility LSN from F4's quorum consensus
- **Output**: Confirms WAL durability before backup starts
- **Contract**: backup cannot start until visibility_lsn >= catalog_snapshot_lsn

### F6 (Restore)
- **Input**: BackupManifest from F5
- **Output**: PITR window [earliest, latest] for target validation
- **Contract**: Restore targets must fall within PITR window or restore fails

### F7 (Audit Traces)
- **Input**: BackupManifestFinalizedEvent from F5
- **Output**: Audit trail for compliance/recovery tracking
- **Contract**: Event must include backup_id, finalized_epoch, PITR window

---

## Failure Modes and Mitigations

| Failure Mode | Cause | Mitigation | Severity |
|---|---|---|---|
| **Backup plan creation fails** | Invalid segment layout | Validation rejects malformed plans early | MEDIUM |
| **Checkpoint not persisted** | Metadata file I/O fails | fsync() confirms durability or raises error | HIGH |
| **Checksum mismatch on resume** | Data corruption during backup | Reject backup; alert operator | CRITICAL |
| **WAL gaps detected** | Missing WAL segment in archive | Backup finalization fails; restore prevented | CRITICAL |
| **I/O budget exceeded** | Concurrent I/O starves backup | Scheduler throttles or pauses tasks | MEDIUM |
| **LSN binding violation** | Plan used with wrong snapshot | Validation rejects mismatched LSN | MEDIUM |

---

## Testing Strategy

### Unit Tests (Inline, in each module)
- Physical plan creation and validation
- Checkpoint persistence and recovery
- Scheduler I/O budgeting
- WAL archive validation

### Contract Tests (backup_physical_plan_contract.rs)
- 21 comprehensive tests covering:
  - Plan creation (2 tests)
  - Segment ordering (3 tests)
  - I/O budgeting (3 tests)
  - Checkpoint lifecycle (4 tests)
  - Checksum validation (2 tests)
  - WAL archive (2 tests)
  - PITR window (2 tests)
  - Error cases (3 tests)

### Integration Tests (TBD: F6 integration)
- BackupManifest → PITR target validation
- Restore planning based on PITR window

---

## Implementation Notes

### File Structure
```
crates/andromeda-storage/src/backup/
├── physical_plan.rs              (BackupPhysicalPlan: pure planning)
├── scheduler.rs                  (BackupIOScheduler: I/O task scheduling)
├── checkpoint_manager.rs         (BackupCheckpointManager: crash safety)
├── wal_archive_integration.rs    (WAL validation + manifest finalization)
├── helpers.rs                    (backup_error() utility)
├── types.rs                      (BackupId, WalArchiveRange, etc.)
├── plan.rs                       (BackupManifest, PITR validation)
├── artifacts.rs                  (Durable artifact structs)
├── execution_plan.rs             (ExtentCopyTask, WalSegmentCopyTask)
└── mod.rs                        (Module exports)

tests/
└── backup_physical_plan_contract.rs  (21 contract tests)

documentations/governance/decisions/
└── DEC-031-f5-backup-physical-plan.md  (This file)
```

### Code Quality Standards
- ✓ No `unsafe` code (forbid(unsafe_code))
- ✓ No ad hoc SQL (type-safe only)
- ✓ Comprehensive error handling (AndromedaResult<T>)
- ✓ Clear invariant documentation
- ✓ All public APIs validated

---

## Acceptance Criteria

- [x] BackupPhysicalPlan struct defined with LSN binding
- [x] BackupIOScheduler implements deterministic scheduling with I/O budgeting
- [x] BackupCheckpointManager enables crash-safe resumption
- [x] WalArchiveIntegration validates WAL coverage and computes PITR windows
- [x] 21 contract tests pass, covering all critical paths
- [x] Module exports configured in backup.rs
- [x] No unsafe code, no SQL generation, no runtime shortcuts
- [x] Decision record documents all invariants and contracts

---

## Sign-Off

**HA/DR and Backup Architect:** Approved  
**Recovery Team Lead:** Review pending (F6 integration)  
**Architecture Review:** Pending B5/B6/F1 confirmation  

**No blockers identified. Ready for code review and integration with F6.**

---

## References

- **Task**: F5 — Plan Physical Backup Execution (durability milestone, Final)
- **Related ADRs**: DEC-020 (HADR Quorum), DEC-024 (WAL Shipping)
- **Doctrine**: Andromeda SGBDRT Master Consolidation 2026
- **Prior Work**: F5_COMPLETE_DELIVERABLES_INDEX.md (BackupExecutionPlan)

---

**Document Version:** 1.0  
**Last Updated:** F5 Completion  
**Status:** ACCEPTED ✓
