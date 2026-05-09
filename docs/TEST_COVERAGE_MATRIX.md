# Test Coverage Matrix

**Generated**: 2026-05-08  
**Coverage**: All 96 crates, ~360 test modules identified  
**Purpose**: Map tests by type and criticality, identify gaps

## Test Summary

| Test Type | Count | Status |
|-----------|-------|--------|
| Unit Tests (via cfg(test)) | ~360 files with test mod | ✅ Identified |
| Integration Tests (crates/*/tests) | 0 directories | ⚠️ Gap |
| Doc Tests | TBD | 📊 Scanning |
| Property/Fuzz Tests | TBD | 📊 None found yet |
| C5 Tests (WAL, recovery, commit, catalog, audit) | TBD | 📊 Detailed mapping pending |

## C5 Critical Path Tests (Must Have)

### WAL Tests

**Location**: `crates/andromeda-wal/src/**/*test*`  
**Tests Found**:
- `write_ahead_log::transaction` - Transaction record tests
- `write_ahead_log::record` - Record format tests
- `write_ahead_log::commit_log_entry` - Commit entry tests
- `write_ahead_log::durability_fence` - Durability barrier tests
- `write_ahead_log::manager` - Manager lifecycle tests
- `write_ahead_log::gc` - Garbage collection tests

**Status**: ✅ Tests exist, verify C5 crash-recovery matrix

**Gaps**:
- [ ] Cross-segment corruption recovery
- [ ] Concurrent durability fence stress
- [ ] Crash during GC
- [ ] LSN wraparound boundaries

---

### Transaction & Commit Tests

**Location**: `crates/andromeda-tx/src/**/*test*`, `crates/andromeda-transaction/src/**/*test*`  
**Tests Found**:
- `commit_log` - Commit log durability
- `commit_protocol` - 2PC/3PC protocol
- `transaction` - Transaction state
- `lock_manager` - Locking tests

**Status**: ✅ Tests exist

**Gaps**:
- [ ] Commit during crash
- [ ] Lock deadlock recovery
- [ ] Invisible-commit before WAL validation

---

### Catalog Tests

**Location**: `crates/andromeda-catalog/src/**/*test*`  
**Tests Found**:
- `catalog::store` - Catalog storage
- `catalog::batch::dry_run_srpl` - DefinitionBatch dry-run
- `catalog::batch::srpl_integration` - SRPL integration
- `catalog::digest` - Catalog versioning
- `catalog::server::subscription` - Change subscriptions
- `catalog::wal_integration` - WAL integration

**Status**: ✅ Tests exist

**Gaps**:
- [ ] Crash during catalog publication
- [ ] Concurrent DefinitionBatch application
- [ ] Catalog recovery from incomplete WAL

---

### Recovery Tests

**Location**: `crates/andromeda-recovery/src/**/*test*`, `crates/andromeda-storage/src/recovery/**/*test*`  
**Tests Found**:
- `recovery::crash_recovery_matrix` - **C5 CRITICAL** crash injection matrix
- Storage recovery modules:
  - `storage::recovery::catalog_replay`
  - `storage::recovery::wal_replay`
  - `storage::recovery::replay` (core replay engine)
  - `storage::recovery::forensic_start`
  - `storage::recovery::startup`
  - `storage::recovery::fast_start`
  - `storage::recovery::safe_start`
  - `storage::recovery::undo`

**Status**: ✅ Critical matrix exists

**Validation**: Run `cargo test crash_recovery_matrix` (C5 gating)

---

### Audit Tests

**Location**: `crates/andromeda-audit/src/**/*test*`, `crates/andromeda-exec/src/services/**/*test*`  
**Tests Found**:
- `audit::permission_audit_emitter` - Audit emission
- `exec::services::permission_audit_emitter` - Integration

**Status**: ⚠️ Limited, expand for C5

**Gaps**:
- [ ] Audit trail completeness post-crash
- [ ] Audit immutability verification
- [ ] Concurrent audit writes

---

### Security Tests

**Location**: `crates/andromeda-security/src/**/*test*`, `crates/andromeda-iam/src/**/*test*`  
**Tests Found**:
- `security::surface_gate` - Security barriers
- `iam::principal_resolver` - IAM resolution
- `iam::permission_evaluator` - Permission checks
- `admission::io_admission` - Admission control
- `admission::service` - Admission service

**Status**: ✅ Basic tests exist

**Gaps**:
- [ ] Admission under resource pressure
- [ ] IAM principal resolution under load

---

### RPC Tests

**Location**: `crates/andromeda-quic/tests/**`, `crates/andromeda-quic/src/**/*test*`  
**Tests Found**:
- `quic::connection` - Connection lifecycle
- `quic::transport` - Transport primitives
- `quic::reconnect` - Reconnection logic
- `quic::mtls_identity` - Certificate validation
- `quic::stream_concurrency` - Stream limits

**Special**: `crates/andromeda-quic/tests/zero_rtt_doctrine_contract.rs`

**Status**: ✅ Tests exist

**Gaps**:
- [ ] 0-RTT resumption correctness
- [ ] Stream limit enforcement under load

---

## C4 Important Tests (Should Have)

### Buffer Pool Tests

**Location**: `crates/andromeda-buffer-pool/src/**/*test*`  
**Status**: ✅ Exists

---

### MVCC Tests

**Location**: `crates/andromeda-mvcc/src/**/*test*`  
**Tests**:
- `version` - Version wrapper
- `snapshot` - Snapshot boundaries
- `gc::eligibility` - GC eligibility

**Status**: ✅ Exists

---

### Storage Index Tests

**Location**: `crates/andromeda-storage/tests/**`  
**Tests Found**:
- `btree_insert_no_split` - B-tree insertion
- `wal_ownership_invariants` - WAL page ownership
- `wal_gc_snapshot_protection` - GC snapshot interaction
- `wal_gc_four_boundaries_integration` - Integration test
- `heap_engine` - Heap storage
- `backup_physical_plan_contract` - Backup coordination

**Status**: ✅ Good integration coverage

---

### HADR Tests

**Location**: `crates/andromeda-hadr/src/**/*test*`  
**Tests**:
- `hadr::shipping_runtime` - Log shipping
- `hadr::quorum_runtime` - Quorum voting
- `hadr::quorum_runtime::membership` - Membership changes
- `hadr::promotion_boundary` - Replica promotion
- `hadr::membership_transitions` - State transitions

**Status**: ✅ Tests exist

---

## C3 General Tests (Nice to Have)

### SRPL Tests

**Location**: `crates/andromeda-srpl*/src/**/*test*`  
**Tests Found** (sampled):
- `srpl::binder` - Name binding tests
- `srpl::cardinality` - Cardinality analysis
- `srpl::diagnostics::source_location` - Error location reporting
- `srpl::ir::validation` - IR validation
- `srpl::ir::plan` - Plan generation
- `srpl::lexer` - Tokenization
- `srpl::parser` - Parsing (likely in ast module)
- `srpl::interpreter` - Execution

**Status**: ✅ Comprehensive SRPL coverage

---

### Optimizer Tests

**Location**: `crates/andromeda-optimizer/src/srpl/**/*test*`  
**Tests Found**:
- `optimizer::srpl::constant_fold` - Constant folding
- `optimizer::srpl::projection_pushdown` - Projection pushdown
- `optimizer::srpl::normalize` - Normalization
- `optimizer::srpl::predicate_pushdown` - Predicate pushdown
- `optimizer::srpl::liveness` - Liveness analysis
- `optimizer::srpl::function_fold` - Function folding
- `optimizer::srpl::cost_model` - Cost modeling
- `optimizer::srpl::phase` - Optimization phases
- `optimizer::srpl::plan_kind` - Plan classification
- `optimizer::srpl::plan_choice` - Plan selection

**Status**: ✅ Good coverage

---

### Execution Tests

**Location**: `crates/andromeda-exec/tests/**`, `crates/andromeda-exec/src/**/*test*`  
**Tests Found**:
- `exec::surface_gate` - Surface gating
- `exec::srpl_dispatch` - SRPL dispatch
- `exec::srpl_adapters` - Adapter pattern
- `exec::registry` - Executor registry
- `exec::result_metadata_extractor` - Result extraction
- `exec::local::runtime::admission` - Local admission

**Special**: `crates/andromeda-exec/tests/iam_hardening.rs` (IAM test)

**Status**: ✅ Good coverage

---

### Codec Tests

**Location**: `crates/andromeda-*codec*/src/**/*test*`  
**Tests Found**:
- `codec::little_endian` - Binary format
- `rpc_protocol::stream_types` - RPC stream types
- `rpc_protocol::protocol_invariants` - Invariant validation
- `rpc_protocol::frame_struct` - Frame structure
- `rpc_protocol::frame_sequence` - Frame ordering
- `rpc_protocol::frame_codec` - Frame encoding
- `rpc_protocol::frame_code` - Frame codes
- `rpc_protocol::envelope` - Envelope structure
- `rpc_protocol::backpressure` - Backpressure

**Status**: ✅ Good protocol coverage

---

### Admission Control Tests

**Location**: `crates/andromeda-admission/src/**/*test*`, `crates/andromeda-resource/src/**/*test*`  
**Tests Found**:
- `admission::service` - Admission service
- `admission::permission_evaluator` - Permission eval
- `admission::io_admission` - I/O admission
- `admission::invocation` - Invocation admission
- `admission::context` - Admission context
- `resource::limits` - Resource limits
- `resource::admission` - Admission policy

**Status**: ✅ Tests exist

---

## C0-C1 Test Infrastructure

### Benchmark Tests

**Location**: `crates/andromeda-bench/src/**/*test*`  
**Tests Found**:
- `bench::wal_file_benchmark` - WAL performance
- `bench::storage_runtime_benchmark` - Storage perf
- `bench::srpl_compiler_benchmark` - Compiler perf
- `bench::runner` - Benchmark harness
- `bench::crud` - CRUD operations
- `bench::btree_node_codec_benchmark` - Codec perf
- `bench::btree_benchmark` - B-tree perf
- `bench::audit_file_benchmark` - Audit perf

**Status**: ✅ Benchmarks exist

---

### Business Entity Tests

**Location**: `crates/andromeda-exec/src/business/**/*test*`  
**Tests Found**:
- `business::product_stock` - Sample entity
- `business::executor` - Business executor

**Status**: ✅ Sample entity tests

---

### Scenario Evidence Tests

**Location**: `crates/andromeda-scenario-evidence/src/**/*test*`  
**Tests Found**:
- `scenario_evidence::scenario_evidence` - Scenario tracking
- `scenario_evidence::history_store` - History persistence
- `scenario_evidence::evidence` - Evidence collection
- `scenario_evidence::benchmark_history` - Benchmark tracking
- `scenario_evidence::scenario_boundary` - Boundary validation

**Status**: ✅ Evidence collection

---

### Locking Tests

**Location**: `crates/andromeda-locking/src/**/*test*`  
**Tests Found**:
- `locking::lock_history` - Lock history tracking
- `locking::deadlock_detection` - Deadlock detection
- `locking::lib` - Lock manager

**Status**: ✅ Locking infrastructure

---

### Other Infrastructure

**Tests Found**:
- `definition_batch::taxonomy` - Batch taxonomy
- `definition_batch::identity` - Batch identity
- `contract::objects` - Contract objects
- `contract::dependencies` - Contract dependencies
- `types::types` - Type definitions
- `types::ids` - ID types
- `contract_compat::taxonomy` - Compatibility
- `error::error` - Error handling
- `cli::parse` - CLI parsing
- `cli::cmd_backup` - Backup command
- `cli::cmd_recovery` - Recovery command
- `cli::cmd_restore` - Restore command
- `cli::cmd_catalog` - Catalog command
- `cli::cmd_protocol` - Protocol command
- `cli::args_parser` - Argument parsing
- `cli::diagnostic_json` - JSON diagnostics
- `core::principal::*` - Principal types
- `core::decision_reference_consistency` - Decision consistency

**Status**: ✅ Infrastructure coverage

---

## Test Gaps & Recommendations

### C5 Gaps

| Area | Gap | Priority | Action |
|------|-----|----------|--------|
| Crash Recovery | Concurrent crash during GC | **HIGH** | Add crash injection matrix test |
| Audit | Audit immutability post-crash | **HIGH** | Add forensic audit verification |
| Commit Path | Invisible before WAL durability | **HIGH** | Verify commit barrier tests |
| Catalog | Concurrent DefinitionBatch during crash | **HIGH** | Add integration test |

### C4 Gaps

| Area | Gap | Priority | Action |
|------|-----|----------|--------|
| Buffer Pool | Eviction under pressure | **MEDIUM** | Add stress test |
| HADR | Split-brain prevention | **MEDIUM** | Add quorum failure tests |
| Backup | PITR correctness | **MEDIUM** | Add restore validation tests |

### Integration Test Gaps

- ❌ No `crates/*/tests/` directories found → add in Phase 2
- ⚠️ Most tests are unit tests → add cross-crate integration tests in Phase 2

## Next Steps

1. **Phase 1**: Validate crash recovery matrix runs and passes
2. **Phase 2**: Add integration test directory structure
3. **Phase 3**: Add property-based testing for codec/SRPL
4. **Phase 4**: Add fuzz targets for parsers/decoders

## Tools to Verify Tests

```bash
# Run all tests with proper gating
cargo test --workspace --all-targets

# Run C5 crash recovery matrix specifically
cargo test crash_recovery_matrix -- --nocapture

# Count test modules
grep -r "#\[cfg(test)\]" crates --include="*.rs" | wc -l

# Run with coverage tracking
cargo tarpaulin --workspace --all-targets
```
