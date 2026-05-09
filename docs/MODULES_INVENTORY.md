# Modules Inventory

**Generated**: 2026-05-08  
**Scope**: All 96 crates, 1,954 Rust files  
**Purpose**: Map all modules by responsibility, identify long files, mark move targets

## Inventory Summary

| Metric | Count | Status |
|--------|-------|--------|
| Total Crates | 96 | ✅ Inventoried |
| Total .rs Files | 1,954 | ✅ Scanned |
| Expected Modules | ~400 | 📊 Pending detailed mapping |
| C5 Modules (Critical) | ~45 | 📊 To mark |
| C4 Modules (Important) | ~80 | 📊 To mark |
| C3+ Modules (General) | ~275 | 📊 To mark |
| Long Files (>1000 LOC) | TBD | 📊 Split candidates |
| Mixed Responsibility | TBD | 📊 Refactor candidates |

## Crate-by-Crate Module Breakdown

### ✅ andromeda-wal (Write-Ahead Log)

**Responsibility**: Transaction durability, log rotation, segment management  
**Criticality**: C5 CRITICAL  
**Decision**: Extract to own top-level crate in Phase 1  

**Modules**:
- `write_ahead_log::manager` - WAL lifecycle, durability fencing
- `write_ahead_log::transaction` - Transaction log entries
- `write_ahead_log::record` - Record format & codec
- `write_ahead_log::record_bounds` - LSN bounds tracking
- `write_ahead_log::commit_log_entry` - Commit record format
- `write_ahead_log::segment_reclaimability` - GC eligibility
- `write_ahead_log::durability_fence` - Durable write barrier
- `write_ahead_log::gc` - Garbage collection
- `write_ahead_log::gc_eligibility` - GC eligibility rules
- `wal_segment` - Segment container
- `lsn` - Log sequence numbers

**Action**: Keep as-is, separate module: `pub mod write_ahead_log` stays (reexport during Phase 1)

---

### ✅ andromeda-storage (Storage Engine & Buffer Pool)

**Responsibility**: Page storage, buffer pool, WAL integration, recovery  
**Criticality**: C5 CRITICAL  
**Decision**: Split in Phase 2-3 (too mixed)  

**Modules**:
- `buffer_pool::manager` - Page cache policy & eviction
- `buffer_pool::clock` - Clock sweep algorithm
- `buffer_pool::dirty` - Dirty page tracking
- `buffer_pool::flush_result` - Flush outcomes
- `write_ahead_log::*` - WAL wrappers (move to andromeda-wal) 
- `btree_*` - B-tree format & validation
- `btree::*` - B-tree operations
- `segment` - Segment container & access
- `manifest` - Segment manifest tracking
- `extent::*` - Extent management
- `backup::*` - Backup integration (move to andromeda-backup)
- `recovery::*` - Recovery paths (move to andromeda-recovery)
- `disk_manager::*` - I/O scheduling
- `cold_store` - Immutable segment store
- `file_wal` - File-based WAL
- `page_codec_v1` - Page encoding
- `format_version` - Format versioning
- `operational_profile` - Performance tracking
- `segment_manifest` - Segment metadata
- `heap_row_encoder` - Row encoding
- `wal_record_catalog` - WAL record type registry
- `catalog_wal_bridge` - Catalog integration
- `restore_orchestration` - Restore coordination

**Action**: Split andromeda-storage in Phase 2+
- Extract: `andromeda-buffer-pool` (independent)
- Extract: `andromeda-recovery` (independent)
- Extract: `andromeda-backup` (independent)
- Keep: Core storage/btree/segment

---

### ✅ andromeda-transaction (Transaction State Machine)

**Responsibility**: Transaction lifecycle, isolation, locking  
**Criticality**: C4-C5  

**Modules**:
- `state` - Transaction states (active, preparing, committed)
- `locking_protocol` - Two-phase locking
- `trace` - Transaction trace events
- `wal_adapter` - WAL integration

**Action**: Keep for now, review for MVCC separation in Phase 2

---

### ✅ andromeda-tx (Transaction Manager & Commit Log)

**Responsibility**: Transaction orchestration, commit protocol  
**Criticality**: C5 CRITICAL  

**Modules**:
- `manager` - Global transaction manager
- `manager_core` - Manager state machine
- `lock_manager` - Lock table management
- `commit_protocol` - 2PC/3PC implementation
- `commit_log` - Commit record durability
- `allocator` - XID allocation

**Action**: Keep as primary transaction orchestrator

---

### ✅ andromeda-mvcc (Multi-Version Concurrency Control)

**Responsibility**: Version snapshot tracking, GC eligibility  
**Criticality**: C4  

**Modules**:
- `version` - Version wrapper type
- `status` - Version status lifecycle
- `snapshot` - Snapshot boundaries
- `active_snapshot_registry` - Live snapshot tracking
- `gc::*` - Garbage collection
- `gc::eligibility` - Eligibility rules
- `gc::mvcc_eligibility` - MVCC-specific eligibility
- `gc::scheduler` - GC scheduling
- `gc::reclamation` - Version reclamation

**Action**: Keep, prepare for async GC in Phase 3

---

### ✅ andromeda-catalog (Catalog Store)

**Responsibility**: Database object registry, versioning, publication  
**Criticality**: C5 CRITICAL  

**Modules**:
- `store` - Catalog backend
- `batch::dry_run_srpl` - DefinitionBatch dry-run
- `batch::srpl_integration` - SRPL integration
- `digest` - Catalog digest/hash
- `fixtures` - Test fixtures
- `wal_integration` - WAL integration
- `statistics::*` - Table statistics
- `procedure_store` - Procedure registry
- `plan_cache` - Cached plans
- `server::subscription` - Change subscriptions
- `server::mod` - Publication server

**Action**: Keep core, extract procedure-store to Phase 2

---

### ✅ andromeda-exec (Execution Engine)

**Responsibility**: Procedure invocation, routing, security checks  
**Criticality**: C4  

**Modules**:
- `dispatch::*` - RPC dispatch
- `dispatch::permission_validation` - IAM checks
- `dispatch::invocation_codec` - Invocation encoding
- `srpl_dispatch` - SRPL execution
- `srpl_adapters` - Adapter pattern for engines
- `surface_gate` - Public surface gating
- `registry` - Executor registry
- `result_metadata_extractor` - Result metadata
- `executor_bridge` - Executor protocol bridge
- `services::permission_audit_emitter` - Audit emission
- `helpers::transaction` - Tx helpers
- `business::*` - Business entity execution
- `local::*` - Local execution context

**Action**: Keep, document service boundaries

---

### ✅ andromeda-srpl (SRPL Language Surface)

**Responsibility**: Language surface, procedure definition  
**Criticality**: C3  

**Modules**:
- `definition_batch_bridge::procedure_definition` - Definition bridge
- Core language = SRPL family (srpl-parser, srpl-ast, srpl-ir, etc.)

**Action**: Keep, link to SRPL family

---

### ✅ andromeda-recovery (Crash Recovery)

**Responsibility**: Safe/fast/forensic startup, consistency verification  
**Criticality**: C5 CRITICAL  

**Modules**:
- `lib` - Recovery coordinator
- `crash_recovery_matrix` - Test matrix (C5 validation)

**Action**: Extract from storage to own crate in Phase 1-2

---

### ✅ andromeda-quic (QUIC Transport)

**Responsibility**: RPC-only network transport, stream management  
**Criticality**: C5 CRITICAL  

**Modules**:
- `transport` - Transport layer
- `connection` - Connection lifecycle
- `stream_concurrency` - Stream limits
- `reconnect::*` - Reconnection logic
- `procedure_gateway` - Procedure routing
- `mtls_identity` - mTLS certificates
- `hadr_streams` - HA/DR replication streams

**Action**: Keep, ensure no non-RPC paths

---

### ✅ andromeda-rpc (RPC Dispatch)

**Responsibility**: RPC routing, method dispatch  
**Criticality**: C4  

**Modules**:
- `dispatch` - Method dispatcher

**Action**: Keep, monitor for SQL surface

---

### ✅ andromeda-security (Security Enforcement)

**Responsibility**: Authorization, admission control, audit  
**Criticality**: C5 CRITICAL  

**Modules**:
- `surface_gate` - Security checkpoints

**Action**: Keep, expand for C5 compliance

---

### ✅ andromeda-audit (Audit Logging)

**Responsibility**: Audit trail emission, event tracking  
**Criticality**: C4-C5  

**Modules**:
- `permission_audit_emitter` - Permission audit
- `lib` - Core audit

**Action**: Keep, expand for comprehensive tracing

---

### ✅ andromeda-iam (Identity & Access Management)

**Responsibility**: Principal identity, role mapping  
**Criticality**: C4  

**Modules**:
- `principal_resolver` - Principal lookup
- `permission_evaluator` - Permission checks
- `lib` - IAM coordinator

**Action**: Keep, review for policy engine separation

---

### ✅ andromeda-codec (Binary Encoding)

**Responsibility**: Typed binary serialization  
**Criticality**: C2  

**Modules**:
- `little_endian` - LE canonical format

**Action**: Keep, ensure no dynamic SQL codecs

---

### ✅ andromeda-types (Core Types)

**Responsibility**: Foundational type definitions  
**Criticality**: C2  

**Modules**:
- `types` - Core type wrappers
- `ids` - Identifier newtypes

**Action**: Keep as foundation, review for splitting in Phase 3

---

### ✅ andromeda-error (Error Types)

**Responsibility**: Error kind taxonomy, codes  
**Criticality**: C2  

**Modules**:
- `error` - Error codes & classification

**Action**: Keep, expand error routing

---

### Long Files Candidates for Splitting

| Crate | File | Est. LOC | Action |
|-------|------|---------|--------|
| andromeda-storage | `lib.rs` + `btree.rs` | 3000+ | Split to storage/btree separation |
| andromeda-optimizer | `srpl/*.rs` | 2000+ | Already split, review boundaries |
| andromeda-catalog | `store.rs` | 2000+ | Split catalog-store path |
| andromeda-exec | `dispatch/*.rs` | 1500+ | Already split, verify imports |
| andromeda-quic | `connection.rs` | 1200+ | Review for connection pool extraction |

## Mixed-Responsibility Modules (Refactor Candidates)

1. **andromeda-storage**: WAL + Buffer Pool + Recovery + Backup → Phase 2 split
2. **andromeda-exec**: Dispatch + SRPL + Business → Keep together, review boundaries
3. **andromeda-catalog**: Store + Batch + Server → Consider split in Phase 3

## Module Dependencies

### Critical Paths

1. **Commit Path**:
   ```
   quic → rpc → exec → transaction → tx → commit_log → wal → storage
   ```

2. **Recovery Path**:
   ```
   recovery → wal_replay → catalog_replay → storage
   ```

3. **Catalog Publication**:
   ```
   catalog → batch → server → quic
   ```

### Forbidden Dependencies

- ❌ Application SQL anywhere in compile
- ❌ Dynamic predicates in catalogs
- ❌ GPU in commit path
- ❌ gRPC (QUIC-only)

## Next Steps

1. **Phase 1**: Extract andromeda-wal, andromeda-recovery
2. **Phase 2**: Split andromeda-storage (buffer-pool, recovery, backup separate)
3. **Phase 3**: Refactor SRPL family, analytics, optimizer placement
4. **Phase 4+**: Polish boundaries, reexport cleanup

## Tools to Verify Modules

```bash
# Check crate boundaries
cargo tree --all-features --depth 1

# Verify no cycles
cargo check --workspace --all-targets

# Lint module boundaries
cargo clippy --workspace --all-targets -- -W warnings

# Count LOC per crate
find crates -name "*.rs" -type f | xargs wc -l | sort -n
```
