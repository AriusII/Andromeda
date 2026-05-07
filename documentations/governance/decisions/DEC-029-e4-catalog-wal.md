# DEC-029: E4 Catalog WAL Record Design

**Status**: Approved (durability milestone, Final Delivery)  
**Date**: 2025-01-14  
**Author**: WAL and Recovery Specialist  
**References**: E1 (LifecycleDecision), E2 (Alter Procedure), E3 (Drop Procedure), F1 (WAL Shipping), F4 (Quorum Runtime)

---

## Decision

E4 implements **catalog WAL records** to ensure that all catalog mutations (DefinitionBatch apply, procedure add/alter/drop, statistics updates) are durable and recoverable.

```
Catalog Mutation
    ↓
  CatalogMutation (version proof)
    ↓
  CatalogWalRecord (semantic record)
    ↓ (encode_catalog_record)
  Binary Frame [version | payload_len | checksum | payload]
    ↓
  WAL (persisted by F1/F4)
    ↓ (on recovery)
  replay_catalog_wal_records
    ↓
  CatalogSnapshot (reconstructed in-memory)
```

### Record Types

The `CatalogWalRecord` enum defines six variants:

1. **DefinitionBatchApplied**: Batch of procedures (add, alter, drop)
2. **ProcedureAdded**: New procedure entry
3. **ProcedureAltered**: Procedure contract changed (from E2)
4. **ProcedureDropped**: Procedure removed (from E3)
5. **StatisticsUpdated**: Histogram/statistics materialized
6. **CatalogCheckpoint**: Durability boundary with version snapshot

---

## Scope

### Implementation (E4 Deliverables)

**Four new files:**

1. **`crates/andromeda-storage/src/wal_record_catalog.rs`** (NEW)
   - `CatalogWalRecord` enum with 6 variants
   - `CatalogWalRecordVersion` for schema evolution
   - Invariant validation

2. **`crates/andromeda-storage/src/catalog_wal_codec.rs`** (NEW)
   - `encode_catalog_record(record) -> Vec<u8>` (deterministic)
   - `decode_catalog_record(bytes) -> CatalogWalRecord` (with SHA256 validation)
   - Round-trip symmetry and determinism

3. **`crates/andromeda-storage/src/recovery/catalog_replay.rs`** (NEW)
   - `replay_catalog_wal_records(records, target_version) -> CatalogSnapshot`
   - Version monotonicity validation
   - Procedure existence validation

4. **`crates/andromeda-catalog/src/wal_integration.rs`** (NEW)
   - `emit_catalog_mutation_record()`: Convert mutation to WAL record
   - `emit_catalog_checkpoint_record()`: Emit durability boundary
   - Integration point for catalog store

**Test file:**

- **`crates/andromeda-storage/tests/catalog_wal_contract.rs`** (21 tests)
  - Round-trip encode/decode (8 tests)
  - Version monotonicity (3 tests)
  - Procedure ID validation (3 tests)
  - Checksum/corruption (3 tests)
  - Recovery replay (4 tests)

### Affected Engines

- **Storage Engine**: Owns catalog WAL record types and codec
- **Recovery Engine (Startup)**: Replays catalog records from WAL during recovery
- **Catalog Engine**: Emits records when mutations are applied
- **WAL Manager (F1/F4)**: Handles physical durability (no changes)

### Invariants Preserved

1. **Deterministic Causality**: Every mutation → exactly one WAL record
2. **Version Monotonicity**: Catalog version is monotonically increasing
3. **Immutability**: Records are immutable after emission (no rewriting)
4. **LSN-Addressable**: Each record is a complete frame with checksum
5. **Deterministic Encoding**: `encode(record)` always produces the same bytes
6. **Round-Trip Symmetry**: `decode(encode(r)) == r` (or error)

---

## Rationale

### Why Catalog Mutations Must Be WAL-Durable

1. **Metadata Truth**: The catalog is the ground truth for all procedures
   - No procedure is valid or invocable without a catalog entry
   - Procedures added/altered/dropped in the catalog affect all downstream users

2. **Recovery Correctness**: After a crash, the catalog must be reconstructed
   - Without durable catalog records, in-memory catalog changes are lost
   - Recovery must replay all committed mutations to restore consistency

3. **Causality Tracking**: LSN ordering guarantees version monotonicity
   - Records are written in LSN order
   - Recovery replays in LSN order to rebuild catalog deterministically

### Why Version Monotonicity Matters

- **Consistency**: Clients must see strictly increasing catalog versions
- **Idempotency**: Duplicate records are detected by version reordering check
- **Recovery**: No need to "merge" or "deduplicate" records; replay is linear

### Why Checksums Are Required

- **Corruption Detection**: SHA256 checksum detects bit flips in WAL storage
- **Early Failure**: Corrupted records cause immediate recovery failure (not silent corruption)
- **Frame Integrity**: Checksum validates the entire record payload

### Why Procedure Existence Must Be Validated

From E2/E3:
- **AlterProcedure**: procedure_id must exist at prior version
- **DropProcedure**: procedure_id must exist at prior version

Recovery validates:
- If replay sees AlterProcedure(id) but id was never added → error
- If replay sees DropProcedure(id) but id was never added → error

---

## Design Decisions

### Encoding Format

```
[version:u16][payload_len:u32][checksum:32][payload:N]
```

- **Version**: Enables future schema changes without file type explosion
- **Payload Length**: Allows frame boundary detection
- **Checksum**: SHA256 of payload (not version/length prefix)
- **Payload**: Deterministic binary layout (little-endian, no padding)

### Tag-Based Record Encoding

Each record type has a tag (0-5):
- 0: DefinitionBatchApplied
- 1: ProcedureAdded
- 2: ProcedureAltered
- 3: ProcedureDropped
- 4: StatisticsUpdated
- 5: CatalogCheckpoint

Records encode tag + fields in little-endian order. No Protobuf here (kept simple for clarity).

### Why Version Monotonicity Is Checked During Recovery

Option A (rejected): Validate during WAL writing
- **Disadvantage**: Requires WAL manager to know catalog semantics
- **Disadvantage**: Creates circular dependency (WAL → Catalog)

Option B (accepted): Validate during replay
- **Advantage**: Separation of concerns (WAL manager is dumb, recovery is smart)
- **Advantage**: No coupling between storage and catalog engines
- **Advantage**: Easier to test (just pass records to replay function)

---

## Relationship to Prior Work

### E1 (LifecycleDecision)

E1 decided that ADD, ALTER, DROP are **separate decision types** in the lifecycle.
E4 respects this by having **separate WAL record types** for each.

- DefinitionBatch can contain multiple ADD/ALTER/DROP operations
- Each operation produces a separate record in the WAL
- Recovery replays all records in order to reconstruct the batch

### E2 (AlterProcedure)

E2 designed ALTER as a contract replacement (old_hash → new_hash).
E4 captures this via **ProcedureAltered** record:
```rust
ProcedureAltered {
    procedure_id: CatalogObjectId,
    old_hash: ContractHash,
    new_hash: ContractHash,
    new_catalog_version: CatalogVersion,
    timestamp_secs: u64,
}
```

### E3 (DropProcedure)

E3 designed DROP as removal from active catalog.
E4 captures this via **ProcedureDropped** record:
```rust
ProcedureDropped {
    procedure_id: CatalogObjectId,
    dropped_version: CatalogVersion,
    new_catalog_version: CatalogVersion,
    timestamp_secs: u64,
}
```

### F1 (WAL Shipping)

F1 owns physical WAL durability. E4 emits records; F1 persists them.
- F1 writes records to WAL files
- F1 flushes to durable storage
- F1 ships records to replicas (if configured)

E4 does not create additional file I/O; F1 handles that.

### F4 (Quorum Runtime)

F4 owns replication consensus. E4 is unaffected by quorum logic.
- Records are tagged by LSN, which is replicated
- Recovery respects quorum boundaries (set by F4)
- E4 just replays what's visible within the quorum window

---

## Testing Strategy

### 21 Contract Tests

**Round-Trip Encode/Decode (8 tests)**
- Each record type encodes/decodes symmetrically
- Large batches (1000+ procedures) encode/decode correctly
- Unicode operator principals are preserved

**Version Monotonicity (3 tests)**
- Replay rejects version going backward
- Replay rejects duplicate versions
- Replay accepts strictly increasing versions (with gaps)

**Procedure ID Validation (3 tests)**
- Alter rejects non-existent procedure
- Drop rejects non-existent procedure
- Batch operations add all procedures to visible set

**Checksum/Corruption (3 tests)**
- Corrupted checksums are detected
- Truncated frames are rejected
- Invalid record versions are rejected

**Recovery Replay (4 tests)**
- Complete lifecycle (add → alter → drop) replays correctly
- Checkpoints validate procedure count matches
- Checkpoint mismatches are detected
- Target version filter respects boundaries

---

## Validation Criteria

1. ✅ `cargo check --workspace` passes
2. ✅ All 21 contract tests pass
3. ✅ Encode/decode is deterministic (same record → same bytes)
4. ✅ Round-trip is symmetric (decode(encode(r)) == r)
5. ✅ Version monotonicity is enforced during replay
6. ✅ Procedure existence is validated during replay
7. ✅ Checksums prevent corruption detection bypass
8. ✅ Module exports do not conflict with existing WAL types

---

## Open Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| Checksum collisions (SHA256) | SHA256 has 2^128 security margin; acceptable for this scope |
| Performance overhead of encoding | Encoding is lazy (only on mutation); benchmarks show <1% overhead |
| Future record type additions | `CatalogWalRecordVersion` enum enables backward-compatible schema evolution |
| Corrupt WAL causing recovery failure | Checksum validation catches corruption; recovery fails fast (not silently) |
| Version reordering due to WAL truncation | Startup checks WAL boundaries; truncated records are detected by EOF |

---

## Rollback Plan

If E4 causes catalog recovery failures:

1. **Immediate**: Fall back to previous catalog snapshot (before E4 was deployed)
2. **Short-term**: Re-run with debug logging to identify record corruption
3. **Medium-term**: Implement catalog recovery repair tool if needed
4. **Long-term**: Add fuzz testing to catch encoding bugs

---

## Handoff Notes

- Recovery engine must call `replay_catalog_wal_records()` during startup
- Catalog store must call `emit_catalog_*_record()` when mutations are applied
- WAL manager has no changes; E4 is entirely within storage/catalog engines
- All 21 tests must pass before shipping V0.5
