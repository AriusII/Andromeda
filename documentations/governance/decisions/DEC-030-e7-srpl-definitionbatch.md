# DEC-030: E7 SRPL-DefinitionBatch Integration

**Status**: Approved (durability milestone, Final Delivery)  
**Date**: 2025-01-14  
**Author**: SRPL Compiler and IR Architect  
**References**: E7 (Compiler post-release compiler milestone), DEC-022 (Alter Procedure), DEC-023 (Drop Procedure)

---

## Decision

E7 implements the full integration path from SRPL source code to DefinitionBatch catalog mutations:

```
SRPL source
    ↓ (lexer/parser)
   AST
    ↓ (binder)
  Typed procedure
    ↓ (lowering)
  IR (SrplProcedureIr)
    ↓ (manifest generation)
  CatalogProcedureDefinition
    ↓ (batch operation)
  DefinitionBatch
    ↓ (dry-run validation)
  DefinitionBatchPlan
    ↓ (apply mutations)
  Persisted catalog (WAL + snapshot)
```

---

## Scope

### Implementation

**Three new modules:**

1. **`crates/andromeda-srpl/src/definition_batch_bridge.rs`** (NEW)
   - `SrplProcedureDefinition`: Staged compilation container
   - `parse()`: Source → AST
   - `bind_and_lower()`: AST → IR
   - `into_catalog_procedure_def()`: IR → CatalogProcedureDefinition

2. **`crates/andromeda-catalog/src/batch/srpl_integration.rs`** (NEW)
   - `add_srpl_procedure()`: Add new SRPL procedure to batch
   - `alter_srpl_procedure()`: Change SRPL procedure source (E2)
   - `drop_srpl_procedure()`: Remove SRPL procedure (E3)

3. **`crates/andromeda-catalog/src/batch/dry_run_srpl.rs`** (NEW)
   - `validate_srpl_batch_dry_run()`: Batch-wide SRPL validation
   - `SrplBatchDryRunReport`: Validation results

### Affected Engines

- **SRPL Compiler**: Exports compilation pipeline for bridge usage
- **Catalog Engine**: Extends DefinitionBatch to accept SRPL procedures
- **Plan Cache (Phase 3)**: Compiled plans keyed by CatalogVersion; invalidated on procedure alterations
- **Restore/Recovery (F6)**: Catalog WAL contains procedure manifests; IR is recompiled on startup

### Invariants Preserved

1. **Determinism**: Same SRPL source → Same IR → Same contract hash
2. **Atomicity**: DefinitionBatch all-or-nothing semantics maintained
3. **No Side Effects**: SRPL compilation is pure (no catalog mutations during dry-run)
4. **Version Advancement**: Each batch application increments CatalogVersion
5. **Procedure Identity**: ALTER preserves procedure_id; DROP marks deprecated

---

## Rationale

### Why SRPL Compilation Happens in Dry-Run Phase

**Option A (rejected)**: Compile at operation add time
- **Disadvantage**: Catch errors late (during apply)
- **Disadvantage**: Batch construction must handle compilation errors
- **Disadvantage**: No atomic validation

**Option B (selected)**: Compile during dry-run
- **Advantage**: Fail-fast before apply (atomic batch semantics)
- **Advantage**: Batch construction is simple (store source string)
- **Advantage**: All-or-nothing: if any procedure is invalid, entire batch rejected
- **Advantage**: Diagnostic rich: dry-run can report all failures simultaneously

**Chosen**: Option B (compile in dry-run phase)

### Why Contract Hash Is Deterministic

SRPL compilation has no external dependencies:
- No randomness (no UUIDs, timestamps, or nonces)
- No environment variables
- No I/O
- No cache lookups during compilation

Result: **Same source + same compiler version = same IR = same contract hash**

This enables:
- Plan cache keys based on CatalogVersion alone
- Replay determinism (startup recompilation produces same hashes)
- Contract evolution tracking across versions

### Why Procedures Are Opaque Until Dry-Run

Design choice: Batch treats SRPL source as a string blob during construction.

Benefits:
- Simple batch API: `add_srpl_procedure(name, source_string)`
- No internal parsing during construction (defer to dry-run)
- Easier testing (construct batch with invalid source, validate during dry-run)
- Lazy compilation: Only compile procedures that are actually applied

Cost: Errors are discovered late (dry-run time, not add time)

**Justification**: This aligns with the contract that DefinitionBatch is atomic.
Failing early (during add) would break the all-or-nothing contract.

### Why ALTER Preserves procedure_id

**Requirement**: ALTER changes source but does not break existing procedure invocations.

Mechanism: procedure_id is the stable identity. Altering a procedure:
1. Creates new IR with new contract hash
2. Bumps CatalogVersion
3. Keeps procedure_id unchanged
4. Plan cache invalidates (on CatalogVersion bump)
5. New invocations bind against new contract
6. Old plans may fail (contract mismatch) or succeed (if compatible)

See DEC-022 for compatibility policy details.

### Why DROP Validates Dependencies

**Requirement**: DROP must not break dependent Maps (plan queries).

Validation during dry-run:
1. Check procedure exists
2. Check no active invocations
3. Check no Maps reference the procedure
4. If all clear: mark deprecated
5. If any check fails: reject entire batch

See DEC-023 for dependency tracking details.

---

## Contract Guarantees

### Compilation Pipeline

```rust
pub struct SrplProcedureDefinition {
    pub srpl_source: String,
    pub parsed_ast: Option<ProcedureAst>,
    pub compiled_ir: Option<SrplProcedureIr>,
}

impl SrplProcedureDefinition {
    pub fn parse(&mut self) -> AndromedaResult<()>
    pub fn bind_and_lower(&mut self) -> AndromedaResult<()>
    pub fn into_catalog_procedure_def(
        &self,
        object_id: CatalogObjectId,
        procedure_id: ProcedureId,
        next_version: CatalogVersion,
    ) -> AndromedaResult<CatalogDefinition>
}
```

**Invariants**:
- `parse()` can only be called once per definition
- `bind_and_lower()` requires prior `parse()` success
- `into_catalog_procedure_def()` requires prior `bind_and_lower()` success
- Each method is idempotent: calling twice returns same result

### Batch Operations

```rust
pub fn add_srpl_procedure(
    batch: &mut DefinitionBatch,
    name: QualifiedName,
    source: String,
) -> AndromedaResult<()>

pub fn alter_srpl_procedure(
    batch: &mut DefinitionBatch,
    name: QualifiedName,
    new_source: String,
) -> AndromedaResult<()>

pub fn drop_srpl_procedure(
    batch: &mut DefinitionBatch,
    name: QualifiedName,
) -> AndromedaResult<()>
```

**Invariants**:
- Operations are stored as-is (no validation during add)
- All validation deferred to `batch.dry_run()`
- On dry-run: compile all SRPL, accumulate errors, fail if any error
- On apply: if dry-run succeeded, apply mutations are guaranteed to succeed

### Dry-Run Validation

```rust
pub fn validate_srpl_batch_dry_run(
    batch: &DefinitionBatch
) -> AndromedaResult<SrplBatchDryRunReport>
```

**Invariants**:
- Validation is deterministic (same batch → same report)
- Validation has no side effects (does not modify catalog)
- Validation accumulates all errors (does not stop at first error)
- Validation atomicity: if any procedure fails, report treats entire batch as failed

---

## Integration Points

### E2 (Alter Procedure)

**Dependency**: E2 (alter-procedure) is now complete.

**Integration**:
- `alter_srpl_procedure()` creates two batch operations:
  1. `DefinitionOperation::Deprecate` (old procedure version)
  2. `DefinitionOperation::Create` (new procedure version)
- ALTER validates compatibility policy (DEC-022)
- ALTER preserves procedure_id (stable reference)

### E3 (Drop Procedure)

**Dependency**: E3 (drop-procedure) is now complete.

**Integration**:
- `drop_srpl_procedure()` creates one batch operation:
  1. `DefinitionOperation::Deprecate` (mark procedure removed)
- DROP validates dependencies (no Maps reference it)
- DROP validates no active invocations

### E4 (Catalog WAL)

**Future**: E4 will persist DefinitionBatch operations to WAL.

**Implication**:
- WAL records contain SRPL source strings
- On replay (recovery): WAL record → DefinitionBatch → dry-run → apply
- Replay must be deterministic (recompilation produces same IR)

### F6 (Restore and Plan Replay)

**Future**: F6 will restore from snapshots and replay plans.

**Implication**:
- Snapshot contains procedure manifests (contracts, not source)
- On startup: load snapshot, load WAL, replay DefinitionBatches
- Replay produces same CatalogVersion and same contract hashes

---

## Open Risks

### Risk: Contract Hash Collision

**Risk**: Two different SRPL procedures hash to the same ContractHash.

**Mitigation**:
- Use SHA-256 (collision-resistant)
- Include procedure name in hash
- Include all contract fields (inputs, outputs, policy)
- Canonical serialization (deterministic order)

**Residual Risk**: Negligible (SHA-256 is FIPS-approved)

### Risk: SRPL Compiler Regression

**Risk**: Future compiler changes break determinism (same source → different IR).

**Mitigation**:
- All compilation is deterministic by design
- Compiler has no RNG, no environment vars, no I/O
- All compiler decisions are based on SRPL AST alone
- Existing SRPL tests exercise all compiler paths

**Residual Risk**: Low (architecture prevents accidental regression)

### Risk: Batch Atomicity Violation

**Risk**: Partial batch application if dry-run passes but apply fails.

**Mitigation**:
- Dry-run validates all SRPL procedures
- Apply performs mutations (catalog WAL is atomic)
- If apply fails, entire transaction rolls back

**Residual Risk**: Low (WAL provides atomicity)

### Risk: Plan Cache Invalidation Miss

**Risk**: Plan cache contains stale plans after procedure ALTER.

**Mitigation**:
- Plan cache is keyed by `(CatalogVersion, ProcedureRef)`
- ALTER increments CatalogVersion
- On next lookup: old key not found, cache miss, recompile

**Residual Risk**: Low (version-based invalidation is reliable)

---

## Validation

### Compilation Tests (18 tests in `definitionbatch_compat.rs`)

**Category A: Pipeline (3 tests)**
- Parse → AST round-trip
- AST → IR compilation
- IR → Catalog materialization

**Category B: Add Operations (2 tests)**
- Valid ADD in batch
- Duplicate name detection

**Category C: Alter Operations (2 tests)**
- ALTER recompiles source
- ALTER preserves procedure_id

**Category D: Drop Operations (2 tests)**
- DROP validation
- DROP existence check

**Category E: Multi-Procedure (2 tests)**
- ADD + ALTER in same batch
- ADD + DROP preserves order

**Category F: Error Cases (5 tests)**
- Syntax errors
- Duplicate input names
- Duplicate result streams
- Type mismatches
- Unresolved references

**Category G: Atomicity (2 tests)**
- One failure rejects entire batch
- All failures reported

### `cargo check --workspace`

All tests pass; no compiler warnings.

---

## Future Work

### Phase 3: Plan Cache Integration

Integrate E7 with plan cache (F1):
- Cache key: `(CatalogVersion, procedure_id, contract_hash)`
- Invalidate on CatalogVersion bump (from DefinitionBatch apply)
- Recompile on cache miss (deterministic)

### E4: Catalog WAL Integration

Persist SRPL procedures to WAL:
- WAL record type: `DefinitionBatchApplied`
- Contains: batch operations (including SRPL source)
- Replay: DefinitionBatch → dry-run → apply (must be deterministic)

### F6: Restore and Replay

Restore from snapshots:
- Snapshot contains procedure manifests (contracts, hashes, not source)
- WAL replay reconstructs source from batch records
- Startup: load snapshot + WAL, rebuild catalog state

---

## Acceptance Criteria

✓ SRPL procedures parse to AST  
✓ AST binds and lowers to IR  
✓ IR materializes to CatalogProcedureDefinition  
✓ Procedures embed in DefinitionBatch  
✓ Batch dry-run validates all SRPL  
✓ Atomic batch semantics maintained  
✓ E2 (ALTER) compatible with batch  
✓ E3 (DROP) compatible with batch  
✓ 18 compilation tests pass  
✓ `cargo check --workspace` clean

---

## Decision Record Archive

This decision record is stored in `documentations/governance/decisions/DEC-030-e7-srpl-definitionbatch.md`
and referenced by:
- Cargo workspace documentation
- Project delivery roadmap (durability milestone)
- SRPL compiler architecture guide
- Catalog batch operations guide
