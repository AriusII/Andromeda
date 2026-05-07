# DEC-038: B-Tree Mutations Deferred to deferred storage milestone; KeyV1 Validation Gate for implementation batch

**Status:** DRAFT → Pending Approval  
**Date:** 2026  
**Task:** implementation batch, Batch 9, Agent 1/4 — B-Tree Format Decision and Validation Gate  
**Author:** Storage Engine Architect  
**Stakeholders:** Storage Engine, WAL/Recovery, Catalog, Buffer Pool, Release Governance  

---

## Decision

B-Tree mutations (insert, delete, split, merge) are **deferred to deferred storage milestone** (storage format v2 era).

implementation batch implements a **KeyV1 validation gate** to prevent format corruption and silent data loss during the mutation deferral period. This gate:

1. **Allows** read-only B-Tree operations (lookup, range_scan) to function
2. **Rejects** mutation attempts (insert, delete) with clear error messages
3. **Enforces** KeyV1 format identity validation before any operation
4. **Prevents** unknown formats from corrupting durable state
5. **Provides** placeholder WAL record types that fail explicitly on apply, preventing silent data loss during recovery

### Formal Scope

| Operation | implementation batch Status | deferred storage milestone Target |
|-----------|-------|---------|
| **lookup()** | ✅ Read-only (no mutations) | Full implementation |
| **range_scan()** | ✅ Read-only (no mutations) | Full implementation with cursor optimization |
| **insert()** | ❌ Deferred, rejects with gate | Full implementation with splits |
| **delete()** | ❌ Deferred, rejects with gate | Full implementation with merges |
| **split()** | ❌ Deferred, placeholder WAL record | Full implementation |
| **merge()** | ❌ Deferred, placeholder WAL record | Full implementation |
| **Index creation** | ⚠️ Deferred or read-only | Full lifecycle (create, insert, maintain, drop) |

---

## Context

### implementation batch Scope

implementation batch delivers the SRPL Execution Runtime and Observability infrastructure (Batches 2-4). The primary deliverables are:
- SRPL concrete adapters (5 implementations)
- Invocation trace wiring and audit events
- Architectural documentation

**B-Tree mutations are not included in implementation batch scope.** This decision record formalizes that deferral and establishes validation gates for safety.

### DEC-032 Prerequisite

DEC-032 ("Storage Format Gate for V1 B-Tree Keys and Heap Pages") established:
- KeyV1 format candidate with locked golden bytes (Null, Int32, Int64, Text, Bytes, Composite)
- Mandatory validation gates before V1 durable promotion
- Requirement that format identity be deterministic and recoverable
- Prohibition on silent format drift

From DEC-032, section "B-Tree key gates":
> "The project must choose one authoritative slot-count and free-space source before durable promotion."
> "Unsupported datum variants must be rejected for durable index keys."
> "Each persisted index must record the key-format identity used to build it."

This decision record implements those gates in implementation batch while deferring mutation implementation.

### Current Implementation State

**crates/andromeda-storage/src/btree.rs** (lines 66-67):
```rust
/// **deferred storage milestone+ Implementation Note:**
/// All methods are design signatures only. Implementations deferred to deferred storage milestone.
pub trait BTreeIndex: Send + Sync { ... }
```

All struct methods contain `todo!()` or `todo!("deferred storage milestone: ...")`:
- `BTreeIndexNode::new_internal()` → `todo!()`
- `BTreeIndexNode::split()` → `todo!("deferred storage milestone: ...")`
- `BTreeIndexNode::merge()` → `todo!("deferred storage milestone: ...")`
- `BTreeRangeCursor::next()` → `todo!("deferred storage milestone: ...")`

This indicates the implementation team has already marked mutations as deferred. This decision record makes that formal.

### Format Versioning Doctrine

From .agents/instructions/VERSIONING_AND_DECISION_RECORDS.md:
> Changes to **WAL record format** require a decision record.

Since placeholder WAL records are being added for B-Tree mutations, a decision record is required. This is it.

### No-Go Rule Compliance

From .agents/instructions/NO_GO_RULES.md, immediate rejection triggers:
- ❌ Application-facing ad hoc SQL
- ❌ gRPC as RPC surface
- ❌ JSON as runtime wire format
- ✅ **Unrecoverable state changes** — This decision *prevents* that by using validation gates

This decision **complies** with the no-go rule prohibiting unrecoverable state changes. Mutation attempts fail explicitly; data is not corrupted.

---

## Consequences

### Positive

1. **Format Stability**: KeyV1 format locked per DEC-032 without mutation pressure to change it
2. **Fail-Fast Doctrine**: Mutation attempts rejected immediately with clear error, not silently ignored
3. **Data Safety**: Placeholder WAL records prevent silent recovery failures
4. **Clear Scope**: Development teams know exactly which operations are deferred
5. **Recovery Clarity**: Recovery can validate format identity before replay; unknown formats rejected
6. **Test Isolation**: Read-only operations can be tested independently from mutation logic
7. **No Breaking Change**: Existing read-only B-Tree code continues to function

### Negative

1. **Incomplete Index Support**: Index modifications not available until deferred storage milestone
2. **Read-Only Limitation**: Dynamic index maintenance deferred
3. **Index Rebuild Cost**: Stale indexes may require full rebuild instead of incremental update
4. **Additional Testing**: Validation gate logic requires 6+ dedicated tests
5. **Manifest Complexity**: Cold storage manifest must track format identity, adding metadata

### Technical Debt

- Index creation logic must be deferred or marked experimental
- Query optimization via indexes limited to read-only plans
- No dynamic index tuning possible during implementation batch

---

## Alternatives Rejected

### Option A: Implement Basic Insert/Delete in implementation batch

**Rejected** because:
- Splits and merges incomplete, leading to tree imbalance
- Format identity not yet validated in DEC-032 verification
- WAL record durability for mutations not yet defined
- Recovery replay for mutations untested
- Buffer pool locking semantics for B-Tree mutations undefined
- Risk of shipping format that must be migrated in deferred storage milestone

### Option B: Revert B-Tree to read-only without validation gates

**Rejected** because:
- No explicit error on mutation attempts → silent failures during recovery
- Unknown formats accepted silently → data corruption risk
- Violates no-go rule: "unrecoverable state changes"
- Recovery team cannot distinguish "mutation not supported" from "format corrupted"

### Option C: Implement full B-Tree in implementation batch

**Rejected** because:
- implementation batch scope is SRPL Execution Runtime (Batches 2-4), not storage mutations
- B-Tree mutations are complex: splits, merges, rebalancing
- DEC-032 validation gates not yet fully implemented
- Resource allocation: Storage team committed to HADR/WAL infrastructure, not B-Tree mutations

### Option D: Implement B-Tree mutations in release gate cycle+ (after current scope)

**Selected + Formalized by this decision record.**

Rationale:
- Allows implementation batch to focus on SRPL execution
- Gives DEC-032 format gates time to mature
- Enables proper testing of splits/merges with recovery
- Allows mutation WAL records to be designed and validated
- Follows project deferral policy (DEC-026)

---

## Validation Plan

### 1. KeyV1 Format Validation Gate

**Invariants**:
- Persisted index includes `key_format_identity` field (major, minor, codec_version)
- Read-only operations (lookup, range_scan) allowed for all known formats
- Mutation operations (insert, delete) rejected with error containing "deferred storage milestone" and "DEC-038"
- Unknown formats rejected before any operation (fail-fast)

**Tests** (6 minimum):
1. ✅ `test_keyv1_read_allowed()` — Lookup and range_scan succeed with KeyV1 format
2. ✅ `test_keyv1_mutation_insert_rejected()` — Insert fails with deferral message
3. ✅ `test_keyv1_mutation_delete_rejected()` — Delete fails with deferral message
4. ✅ `test_unknown_format_rejected()` — Format validation gate rejects codec_version=99
5. ✅ `test_format_identity_deterministic()` — Multiple validators produce identical results
6. ✅ `test_recovery_format_check_before_replay()` — FastStart fails on format mismatch

**Acceptance**: All 6+ tests pass; `cargo test -p andromeda-storage btree_format_deferral_gate -- --nocapture` shows no failures.

### 2. Placeholder WAL Records

**Invariants**:
- BTreeWalRecord enum includes Insert, Delete, Split, Merge variants
- Each variant marked with comment "deferred to deferred storage milestone"
- apply() method returns explicit Err(Deferred) error
- Error message includes reference to "DEC-038"

**Tests** (3 minimum):
1. ✅ `test_btree_wal_insert_placeholder_fails()` — Record can be decoded but apply() fails
2. ✅ `test_btree_wal_recovery_rejects_mutation()` — Recovery replay fails on B-Tree mutations
3. ✅ `test_wal_round_trip_placeholder_preserved()` — Placeholder records survive WAL scan

**Acceptance**: All tests pass; WAL round-trip test shows placeholder records are scanned but not applied.

### 3. Documentation

**Acceptance Criteria**:
- ✅ DEC-038 decision record exists and approved
- ✅ B-Tree module documentation updated with "deferred storage milestone" deferral note
- ✅ Recovery documentation states "B-Tree mutations deferred; implementation batch is read-only"
- ✅ All mutation rejection error messages reference DEC-038
- ✅ Format validation specification document created

### 4. Integration with Existing Tests

**Existing Tests to Verify**:
- `btree_key_codec_contract.rs` — Golden vectors unchanged; KeyV1 locked
- `property_btree_ops.rs` — Mock B-Tree remains valid
- `btree_invariants_e2e.rs` — Read-only invariants hold

**Result**: All existing B-Tree tests pass without modification.

---

## Implementation Tasks

### implementation batch Deliverables (This Work Item)

1. **DEC-038 Decision Record** ✅ (This document)
   - Decision clearly stated
   - Consequences and alternatives documented
   - Validation plan outlined

2. **KeyV1 Format Validation Gate** (New file)
   - File: `crates/andromeda-storage/src/btree_format_validation.rs`
   - Implement KeyV1FormatValidator struct
   - Validate format identity before operations
   - Reject mutations with clear deferral message

3. **Placeholder WAL Records** (Edit existing)
   - File: `crates/andromeda-storage/src/wal_record_catalog.rs`
   - Add BTreeWalRecord enum variants
   - Implement apply() to reject with deferral reference
   - Add version field to track format compatibility

4. **Comprehensive Tests** (New file)
   - File: `crates/andromeda-storage/tests/btree_format_deferral_gate.rs`
   - 6+ tests as per validation plan
   - Test both happy path (read) and sad path (mutations)
   - Test recovery behavior

5. **Documentation Updates**
   - Update `btree.rs` module documentation
   - Add comment: "See DEC-038 for deferred storage milestone deferral rationale"
   - Update recovery guides with B-Tree format validation note

### deferred storage milestone+ Follow-up Work

1. **B-Tree Mutation Implementation**
   - Implement insert() with node splits
   - Implement delete() with node merges
   - Rebalancing logic

2. **WAL Record Implementation**
   - Implement BTreeInsert, BTreeDelete, BTreeSplit, BTreeMerge apply() methods
   - Integration with recovery replay

3. **Recovery Validation**
   - Remove format-unknown rejection; replace with full format support
   - Replay B-Tree mutations during recovery
   - Verify tree invariants after replay

---

## Rollback Plan

If B-Tree mutations must be enabled before deferred storage milestone (emergency override):

1. **Approval Required**: DEC-038 must be explicitly rescinded by Release Governance
2. **Re-plan implementation batch**: Remove 1 SRPL scope item to make room for B-Tree mutations
3. **DEC-032 Completion**: Implement all DEC-032 validation gates (heap page, format identity storage, recovery rejection of unknown formats)
4. **Testing**: 20+ additional tests covering splits, merges, rebalancing, recovery
5. **Recovery Validation**: Full crash-recovery testing with B-Tree mutations
6. **Format Lock**: Commit to KeyV1 format with no breaking changes in v1.x releases

---

## Open Questions

1. **Index Creation**: Should index creation be deferred to deferred storage milestone, or can indexes be created as read-only during implementation batch?
   - **Resolution**: Deferred to implementation team. Recommend marking index creation as experimental/read-only if attempted before deferred storage milestone.

2. **Query Optimization**: Can read-only indexes be used for query optimization during implementation batch?
   - **Resolution**: Yes, if query optimizer can build indexes itself. Otherwise, deferred to deferred storage milestone.

3. **Backward Compatibility**: If implementation batch creates indexes, can deferred storage milestone mutations work on them?
   - **Resolution**: Yes, if format identity is recorded correctly. Validate identity on deferred storage milestone mutation startup.

4. **Index Rebuild**: If an old index format becomes unsupported, can it be automatically rebuilt?
   - **Resolution**: Yes, but only if it's marked as derived (not durable). Schema and heap remain; index is derived. Defer to deferred storage milestone design.

---

## Related Decision Records

- **DEC-026**: Release Gates and Deferral Policy
- **DEC-032**: Storage Format Gate for V1 B-Tree Keys and Heap Pages
- **DEC-033**: durability milestone Durable Audit Ledger
- **DEC-034**: durability milestone V1.0 Production Ready

---

## Doctrine Checks

### No-Go Rule Compliance

✅ **Unrecoverable state changes**: COMPLIANT
- Validation gates prevent silent corruption
- Mutation attempts fail explicitly
- Format identity validated before recovery replay

✅ **Silent type conversions**: COMPLIANT
- Format validation explicit and audited
- No implicit format coercions

✅ **Hidden mutable global state**: COMPLIANT
- Validation gate is immutable after creation
- Format identity persisted in index metadata

### Durability Doctrine

✅ **WAL before visible commit**: COMPLIANT
- Placeholder records preserve WAL ordering
- Mutations deferred until full implementation

✅ **Format versioning deterministic**: COMPLIANT
- KeyV1 golden vectors locked per DEC-032
- Format identity persisted with index
- Validator produces deterministic results

### Recovery Doctrine

✅ **Recovery rejects unknown formats**: COMPLIANT
- Validation gate fails fast on format mismatch
- ForensicStart allowed; FastStart/SafeStart reject unknown formats

---

## Sign-Off

| Role | Recommendation | Status |
|------|-----------------|--------|
| **Storage Engine Architect** | APPROVE DEC-038 deferral + validation gate | 🔄 Pending |
| **WAL/Recovery Specialist** | APPROVE placeholder records + validation | 🔄 Pending |
| **Release Governance** | APPROVE implementation batch scope without B-Tree mutations | 🔄 Pending |
| **Quality Lead** | APPROVE validation plan (6+ tests) | 🔄 Pending |

---

## Appendix: Implementation Checklist

- [ ] DEC-038 approved by storage leadership
- [ ] KeyV1 format validation gate implemented (btree_format_validation.rs)
- [ ] Placeholder WAL records added to wal_record_catalog.rs
- [ ] 6+ tests created in btree_format_deferral_gate.rs
- [ ] B-Tree module documentation updated with DEC-038 reference
- [ ] Recovery documentation updated with B-Tree format validation note
- [ ] All existing B-Tree tests pass without modification
- [ ] `cargo test -p andromeda-storage btree` passes
- [ ] `cargo test -p andromeda-storage btree_format_deferral_gate -- --nocapture` shows green
- [ ] Code review completed by Storage + Recovery teams
- [ ] Decision record linked in project tracking system

---

**Document Version**: 1.0  
**Last Updated**: 2026-01-XX  
**Next Review**: deferred storage milestone implementation planning
