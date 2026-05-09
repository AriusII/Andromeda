# Criticality Matrix

**Generated**: 2026-05-08  
**Purpose**: Classify all 96 crates by mission criticality (C0-C5)  
**Impact**: Determines testing rigor, review process, release gating

## Criticality Levels

| Level | Meaning | Definition | Testing | Review | Gate |
|-------|---------|-----------|---------|--------|------|
| **C5** | CRITICAL | Commit path, WAL, recovery, catalog, security | Crash matrix, fuzz | 2+ experts | Release blocker |
| **C4** | IMPORTANT | Transaction, buffer pool, HADR, admission | Integration, property | 1+ expert | Pre-release gate |
| **C3** | GENERAL | Optimizer, SRPL, analytics, execution | Unit + property | 1 reviewer | Standard CI |
| **C2** | SUPPORTING | Types, codec, protocols, utilities | Unit tests | 1 reviewer | Standard CI |
| **C1** | FOUNDATIONAL | Error types, hardware profiles, resources | Unit tests | 1 reviewer | Standard CI |
| **C0** | TOOLING | CLI, benchmarks, test infrastructure | Ad hoc | N/A | Best effort |

---

## C5: Mission-Critical (Commit, WAL, Recovery, Catalog, Security)

### Commit Path Crates

| Crate | Responsibility | Evidence | Justification |
|-------|-----------------|----------|---|
| `andromeda-tx` | Transaction orchestration & 2PC | Every OLTP write | Must not lose committed data |
| `andromeda-transaction` | State machine & locking | Every OLTP write | Data isolation, deadlock prevention |
| `andromeda-quic` | RPC transport (only) | Every network call | Protocol reliability, no packet loss without detection |
| `andromeda-rpc` | RPC dispatch & routing | Every request | Correct method routing |
| `andromeda-exec` | Procedure invocation | Every OLTP write | Correct procedure execution |
| `andromeda-security` | Authorization enforcement | Every request | Access control, audit trail |

### WAL Path Crates

| Crate | Responsibility | Evidence | Justification |
|-------|-----------------|----------|---|
| `andromeda-wal` | Durable log implementation | Every commit | No data loss, LSN ordering |
| `andromeda-storage` | Page persistence, buffer pool | Every write | Page durability, no corruption |

### Recovery Path Crates

| Crate | Responsibility | Evidence | Justification |
|-------|-----------------|----------|---|
| `andromeda-recovery` | Crash recovery orchestration | Crash shutdown + restart | Consistency after crash, ACID verification |
| `andromeda-catalog-recovery` | Catalog replay from WAL | Crash recovery | Catalog consistency |

### Catalog Publication Crates

| Crate | Responsibility | Evidence | Justification |
|-------|-----------------|----------|---|
| `andromeda-catalog` | Catalog store & versioning | DDL operations | Schema consistency, DefinitionBatch atomicity |
| `andromeda-definition-batch` | Batch procedure import | Schema changes | Transaction isolation for DDL |

### Security & Audit Crates

| Crate | Responsibility | Evidence | Justification |
|-------|-----------------|----------|---|
| `andromeda-audit` | Audit trail emission | Security compliance | Immutable audit log, no gaps |
| `andromeda-iam` | Identity & access | Every request | Principal mapping, permission evaluation |

---

## C4: Important (Not on Critical Path, But Failure Loses Data/Consistency)

### Transaction Infrastructure

| Crate | Responsibility | Evidence |
|-------|-----------------|----------|
| `andromeda-mvcc` | MVCC snapshots, GC | Visibility boundaries, GC eligibility |
| `andromeda-locking` | Lock manager, deadlock detection | Prevents lock leaks, detects cycles |
| `andromeda-tx` (commit log) | Durable commit record storage | Commit boundary verification |

### Storage Infrastructure

| Crate | Responsibility | Evidence |
|-------|-----------------|----------|
| `andromeda-buffer-pool` | Page cache, eviction policy | Cache coherence, no stale reads |
| `andromeda-manifest` | Segment metadata tracking | Manifest integrity |
| `andromeda-backup` | Backup orchestration | PITR correctness |
| `andromeda-restore` | Point-in-time restore | Restore consistency |
| `andromeda-procedure-store` | Procedure registry & history | Procedure availability |
| `andromeda-definition-batch` | Batch operations | Schema atomicity |

### RPC Infrastructure

| Crate | Responsibility | Evidence |
|-------|-----------------|----------|
| `andromeda-rpc-protocol` | RPC frame & envelope protocol | Frame integrity, stream sequencing |
| `andromeda-quic-runtime-quinn` | Quinn QUIC backend | Connection reliability, stream ordering |

### HA/DR

| Crate | Responsibility | Evidence |
|-------|-----------------|----------|
| `andromeda-hadr` | Replication, failover | Replica consistency, split-brain prevention |

### Admission & Resource

| Crate | Responsibility | Evidence |
|-------|-----------------|----------|
| `andromeda-admission` | Admission control gating | Prevents overload, OOM protection |
| `andromeda-resource` | Resource limit definitions | Budget enforcement |

### Catalog Support

| Crate | Responsibility | Evidence |
|-------|-----------------|----------|
| `andromeda-catalog-store` | Catalog storage abstraction | Storage layer correctness |
| `andromeda-catalog-diff` | Catalog mutation tracking | Change tracking completeness |

---

## C3: General (Affects Performance/Features, Not Data Integrity)

### Query Processing

| Crate | Responsibility | Evidence |
|-------|-----------------|----------|
| `andromeda-optimizer` | Query cost modeling & plan selection | Plan quality, no pathological plans |
| `andromeda-srpl-*` (family) | SRPL language surface, parsing, IR | Language semantics, correct lowering |
| `andromeda-plan-cache` | Cached query plans | Plan cache hits, invalidation |

### Execution

| Crate | Responsibility | Evidence |
|-------|-----------------|----------|
| `andromeda-exec` | Procedure execution runtime | Correct method dispatch |
| `andromeda-execution` | Execution plan runner | Plan execution |
| `andromeda-procedure-runtime` | Procedure invocation runtime | Procedure context management |
| `andromeda-srpl-execution-adapter` | SRPL → execution adapter | SRPL execution integration |

### Analytics & Statistics

| Crate | Responsibility | Evidence |
|-------|-----------------|----------|
| `andromeda-statistics` | Statistics collection | Cardinality estimation accuracy |
| `andromeda-analytics` | Analytics query layer | Analytical workload support |
| `andromeda-maps` | Materialized views/maps | Map consistency |

### Storage Internals

| Crate | Responsibility | Evidence |
|-------|-----------------|----------|
| `andromeda-storage-page` | Page format & codec | Page integrity |
| `andromeda-storage-index` | B+Tree structures | Tree balance, key ordering |
| `andromeda-storage-heap` | Heap page operations | Row storage, slot directory |
| `andromeda-segment` | Segment management | Segment boundaries |
| `andromeda-disk-page-store` | Physical page I/O | I/O correctness |

### Protocol Support

| Crate | Responsibility | Evidence |
|-------|-----------------|----------|
| `andromeda-result-stream` | Streaming result protocol | Result transmission |
| `andromeda-rpc-codec` | RPC message encoding | Message serialization |

---

## C2: Supporting (Infrastructure, Not Direct Data Impact)

| Crate | Responsibility | Testing | Justification |
|-------|-----------------|---------|---|
| `andromeda-types` | Core type definitions | Unit | Type safety only |
| `andromeda-core` | Engine abstractions | Unit | Trait definitions |
| `andromeda-contract` | Contract surface abstractions | Unit | Type wrapping |
| `andromeda-error` | Error kind taxonomy | Unit | Error classification |
| `andromeda-codec` | Binary codec utilities | Unit + fuzz | Message correctness |
| `andromeda-proto` | Protobuf message definitions | Unit | Message schema |
| `andromeda-proto-wire` | Protobuf wire layer | Unit + fuzz | Wire protocol |
| `andromeda-procedure-contract` | Procedure typing | Unit | Type contracts |
| `andromeda-observability` | Observability types | Unit | Trace types |
| `andromeda-observe` | Event emitter/tracer | Unit | Event emission |
| `andromeda-decision-trace` | Decision trace logging | Unit | Trace collection |
| `andromeda-execution-trace` | Execution trace types | Unit | Trace types |
| `andromeda-security-contract` | Security API surface | Unit | Type contracts |
| `andromeda-structured-object` | Typed object storage | Unit | Object encoding |
| `andromeda-vector` | Vector type support | Unit | Vector storage |
| `andromeda-retry` | Retry policies | Unit | Retry logic |
| `andromeda-time` | Time utilities | Unit | Time handling |
| `andromeda-digest` | Hashing utilities | Unit | Hash correctness |
| `andromeda-columnar` | Columnar storage format | Unit | Format correctness |
| `andromeda-hardware` | Hardware profile detection | Unit | Profile accuracy |
| `andromeda-scenario-evidence` | Benchmark evidence | Unit | Evidence tracking |

---

## C1: Foundational (Base Infrastructure)

| Crate | Responsibility | Testing | Justification |
|-------|-----------------|---------|---|
| `andromeda-forensic` | Forensic startup diagnostics | Unit | Diagnostic only |
| `andromeda-gpu` | GPU analytics (off commit) | Unit | Analytics only, gated off |
| `andromeda-simd` | SIMD kernels | Unit + microbench | Performance only |
| `andromeda-policy` | Authorization policy evaluation | Unit | Policy logic |
| `andromeda-transaction-log` | Transaction log storage | Unit | Log storage |
| `andromeda-contract-compat` | Legacy compatibility | Unit | Compatibility shims |
| `andromeda-admin` | Admin operations | Unit | Admin tools |

---

## C0: Tooling (Not Production-Critical)

| Crate | Responsibility | Testing | Justification |
|-------|-----------------|---------|---|
| `andromeda-cli` | Command-line interface | Ad hoc | CLI tool |
| `andromeda-bench` | Benchmark suite | Smoke | Benchmarking |
| `andromeda-bench-harness` | Benchmark harness | Smoke | Benchmark support |
| `andromeda-bench-workload` | Workload generation | Smoke | Workload creation |
| `andromeda-regression` | Regression detection | Smoke | Regression analysis |
| `andromeda-test-support` | Test fixtures & helpers | N/A | Test infrastructure |
| `andromeda-business-fixtures` | Business entity fixtures | N/A | Test data |
| `andromeda-srpl-test-fixtures` | SRPL test fixtures | N/A | Test data |
| `andromeda-runbooks` | Runbook documentation | N/A | Documentation |
| `andromeda-client-sdk-gen` | SDK code generation | N/A | Tooling |

---

## Testing Requirements by Criticality

| Level | Unit | Integration | Property | Fuzz | Crash | Coverage |
|-------|------|-------------|----------|------|-------|----------|
| **C5** | ✅ Required | ✅ Required | ✅ Required | ✅ Required | ✅ Required | ≥85% |
| **C4** | ✅ Required | ✅ Required | ✅ Recommended | 📊 Planned | 📊 Planned | ≥75% |
| **C3** | ✅ Required | 📊 Planned | 📊 Recommended | ⚠️ As needed | ❌ No | ≥60% |
| **C2** | ✅ Required | ❌ No | ❌ No | ⚠️ As needed | ❌ No | ≥50% |
| **C1** | ✅ Required | ❌ No | ❌ No | ❌ No | ❌ No | ≥40% |
| **C0** | ⚠️ Smoke | ❌ No | ❌ No | ❌ No | ❌ No | N/A |

---

## Review Requirements by Criticality

| Level | Approvers | Code Review | Design Review | Security Review | Performance Review |
|-------|-----------|------------|----------------|-----------------|-------------------|
| **C5** | 2+ | ✅ Mandatory | ✅ Mandatory | ✅ Mandatory | ✅ Mandatory |
| **C4** | 1+ | ✅ Mandatory | ✅ Recommended | ✅ If security-related | 📊 If performance-critical |
| **C3** | 1+ | ✅ Standard | 📊 Recommended | ⚠️ If security-relevant | ❌ No |
| **C2** | 1+ | ✅ Standard | ❌ No | ⚠️ If security-relevant | ❌ No |
| **C1** | 1+ | ✅ Standard | ❌ No | ❌ No | ❌ No |
| **C0** | N/A | Best effort | ❌ No | ❌ No | ❌ No |

---

## Release Gating by Criticality

| Level | Pre-Release Gate | Release Gate | Post-Release Monitoring |
|-------|-----------------|-------------|------------------------|
| **C5** | All tests pass + crash matrix | Extended soaked run (24h) | Continuous |
| **C4** | Integration tests pass | Standard release process | Monitored |
| **C3** | CI green | Standard release process | Monitored |
| **C2** | CI green | Standard release process | As needed |
| **C1** | CI passes | Standard release process | As needed |
| **C0** | Best effort | Best effort | N/A |

---

## Tools to Enforce Criticality

### Cargo Feature Flags

```toml
# Cargo.toml for C5 crates
[features]
default = []
fuzz = []
crash-recovery-matrix = []
c5-verification = ["fuzz", "crash-recovery-matrix"]
```

### CI Gates

```yaml
# .github/workflows/gates.yml
jobs:
  c5-gate:
    runs-on: ubuntu-latest
    steps:
      - name: C5 Verification
        run: |
          cargo test --workspace --features c5-verification
          cargo fuzz run --timeout 60 fuzz_target_*
```

---

## Next Steps

1. **Phase 0**: Lock criticality matrix
2. **Phase 1**: Enforce C5 testing requirements
3. **Phase 2**: Implement C4 integration tests
4. **Phase 3+**: Expand property-based testing

## References

- Test Matrix: `docs/TEST_COVERAGE_MATRIX.md`
- Fuzz Targets: `fuzz/README.md`
- Release Gates: `docs/testing/release-gates.md`
