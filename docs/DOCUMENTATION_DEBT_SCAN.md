# Documentation Debt Scan

**Generated**: 2026-05-08  
**Scope**: All 96 crates, rustdoc, doc comments, markdown docs  
**Purpose**: Identify stale/redundant documentation, plan migration to ADRs

## Summary

| Issue | Count | Severity | Action |
|-------|-------|----------|--------|
| Over-commented code (paraphrasing logic) | TBD | **MEDIUM** | Reduce to "why", not "what" |
| Stale rustdoc (obsolete after consolidation) | TBD | **LOW** | Update post-Phase 2 |
| Historical context in code | TBD | **MEDIUM** | Move to ADR + HISTORY.md |
| C5 modules missing rustdoc | TBD | **HIGH** | Complete in Phase 1 |
| Public API undocumented | TBD | **HIGH** | Complete in Phase 2 |
| Design decisions in comments | TBD | **MEDIUM** | Extract to docs/adr/ |

## Documentation Debt Categories

### 🔴 CRITICAL - Complete Before Phase 1

**Requirement**: Every C5 crate must have complete rustdoc for public API

| Crate | Status | Gap | Action |
|-------|--------|-----|--------|
| andromeda-wal | ⚠️ Partial | Document manager, LSN, durability fence | Add #[doc] with invariants |
| andromeda-transaction | ⚠️ Partial | State machine transitions | Add state diagram |
| andromeda-tx | ⚠️ Partial | Commit protocol steps | Add 2PC/3PC explanation |
| andromeda-quic | ⚠️ Partial | Stream protocol, reconnection | Document admission |
| andromeda-catalog | ⚠️ Partial | DefinitionBatch semantics | Document publication safety |
| andromeda-recovery | ⚠️ Partial | Startup modes, recovery invariants | Add recovery plan docs |
| andromeda-security | ⚠️ Partial | Security barriers, audit requirements | Document security model |
| andromeda-storage | ⚠️ Partial | Page format, manifest semantics | Add storage layout docs |

---

### 🟠 HIGH - Complete in Phase 1-2

**Requirement**: C4 crates should have documented public API

```rust
// Example: Missing rustdoc before
pub fn commit(&mut self) -> Result<()> {
    // Implementation
}

// After: Clear intent and safety properties
/// Commits this transaction to the WAL.
///
/// # Safety Properties
/// - Writes to WAL before visibility in MVCC snapshot
/// - Atomic: either fully committed or fully aborted (no partial visibility)
/// - Idempotent: calling commit twice returns error on second call
///
/// # Errors
/// - `TransactionAborted` if transaction was previously aborted
/// - `DurabilityFault` if WAL write failed
pub fn commit(&mut self) -> Result<()> {
    // Implementation
}
```

---

### 🟡 MEDIUM - Address in Phase 2-3

**Issue**: Historical context should move to ADRs, not stay in code

**Examples of Over-Documentation**:
```rust
// ❌ ANTI-PATTERN: Explain decision, not code
// We use column-oriented storage for analytics because row-oriented
// was too slow for aggregate queries. This was measured in Q3 2025
// benchmarks. See benchmark_q3_2025.csv for details.
pub struct ColumnStore { ... }

// ✅ PATTERN: Why is this needed (briefly), link to ADR
/// Columnar storage optimized for analytic workloads.
/// See ADR-042 for cost-benefit analysis vs row-oriented.
pub struct ColumnStore { ... }
```

**Remediation**:
- Extract long explanations to `docs/adr/ADR-xxx.md`
- Keep code comments to 1-2 sentences
- Link from code to ADR with `/// See ADR-xxx` pattern

---

### 🟢 LOW - Nice to Have in Phase 3+

**Issue**: Redundant comments after module consolidation

**Examples**:
```rust
// ❌ REDUNDANT: This function does exactly what its name says
/// Returns the transaction ID.
pub fn transaction_id(&self) -> TransactionId {
    self.xid
}

// ✅ CORRECT: Only document non-obvious behaviors
/// Returns the transaction ID if this connection has an active transaction.
pub fn transaction_id(&self) -> Option<TransactionId> {
    self.xid
}
```

---

## Specific Findings

### C5 Crate Documentation Gaps

#### andromeda-wal

**Current State**:
- ✅ `write_ahead_log::manager` - documented
- ⚠️ `lsn` - minimal documentation
- ⚠️ `durability_fence` - unclear semantics
- ⚠️ WAL segment lifecycle - not documented

**Action**:
```rust
// Add to crate root
//! # Write-Ahead Log
//!
//! This module implements Andromeda's durable transaction log.
//!
//! ## Invariants
//! - No visible commit without LSN < durable_boundary
//! - No concurrent WAL writes (mutex protected)
//! - No record loss before durable_boundary
//!
//! ## Safety
//! - All writes serialized through manager
//! - Durability fence enforced before visibility
//! - Segment rotation atomic
```

---

#### andromeda-transaction

**Current State**:
- ⚠️ State machine transitions unclear
- ⚠️ Locking protocol not documented
- ✅ Lock manager has some docs

**Action**: Add state diagram in rustdoc module-level comment

---

#### andromeda-recovery

**Current State**:
- ⚠️ Startup modes (Fast, Safe, Forensic) not clearly distinguished
- ⚠️ Consistency verification steps missing
- ⚠️ Recovery plan not documented

**Action**: Add recovery plan documentation as module-level doc

---

### Historical Context to Extract

| Location | Issue | Extract To |
|----------|-------|------------|
| `andromeda-optimizer/src/srpl/cost_model.rs` | Long comment explaining model choices | ADR-041 |
| `andromeda-storage/src/buffer_pool/manager.rs` | Comment about clock sweep algorithm rationale | ADR-030 |
| `andromeda-quic/src/connection.rs` | Comment about 0-RTT resumption strategy | ADR-038 |
| `andromeda-catalog/src/batch/dry_run_srpl.rs` | Long explanation of DefinitionBatch semantics | ADR-033 |

---

### Design Decisions in Code (Should Be ADRs)

These long comments should become ADRs:

1. **Page size justification** (if in storage docs)
   - ADR: `docs/adr/ADR-036-page-size-2026.md`
   - Keep in code: Link only

2. **ContractHash canonicalization** (if in contract-compat)
   - ADR: `docs/adr/ADR-040-contract-hash.md`
   - Keep in code: Link only

3. **MVCC GC eligibility rules** (if in mvcc/gc)
   - ADR: `docs/adr/ADR-034-mvcc-gc-eligibility.md`
   - Keep in code: Link only

---

## Remediation Strategy

### Phase 0 (Current)

- ✅ Identify documentation gaps
- 📊 Prioritize C5 crates
- 📊 Extract decision contexts

### Phase 1 (Weeks 3-4)

- 🔄 Add rustdoc to all C5 public APIs
- 🔄 Document state machines (diagrams in markdown)
- 🔄 Extract historical context to ADRs

### Phase 2 (Weeks 5-6)

- 🔄 Complete C4 rustdoc
- 🔄 Reduce over-commented code blocks
- 🔄 Verify pub use reexports are documented

### Phase 3+ (Weeks 7+)

- 🔄 Update rustdoc after module moves
- 🔄 Clean up redundant comments
- 🔄 Final documentation audit

---

## Documentation Standards

### Required Format

**For C5 Crate Public API**:

```rust
/// Short description (one line).
///
/// Long description explaining why this exists and when to use it.
///
/// # Safety / Invariants
/// - List of guaranteed properties
/// - No property is assumed, all must be proven
///
/// # Errors
/// - `ErrorKind::Variant` - when this occurs
/// - List all error cases
///
/// # Example
/// ```no_run
/// let result = operation()?;
/// ```
///
/// # See Also
/// - Related function
/// - ADR-xxx for design rationale
pub fn operation() -> Result<Value> { ... }
```

### For State Machines

Include ASCII diagram:

```rust
//! # Transaction State Machine
//!
//! ```text
//! Active ──(prepare)--> Preparing ──(commit)--> Committed
//!   |                      |
//!   └─────(abort)--------> Aborted
//! ```
```

---

## Tools for Documentation Audit

### Find under-documented C5 crates

```bash
for crate in andromeda-wal andromeda-transaction andromeda-quic andromeda-recovery; do
  echo "=== $crate ===" 
  grep -r "^pub " crates/$crate/src/lib.rs crates/$crate/src/*.rs \
    | grep -v "^///\|^//!" | head -10
done
```

### Find over-commented files

```bash
# Files with >30% of lines being comments
find crates -name "*.rs" -type f | while read f; do
  total=$(wc -l < "$f")
  comments=$(grep "^\s*//" "$f" | wc -l)
  pct=$((100 * comments / total))
  [ "$pct" -gt 30 ] && echo "$f: ${pct}%"
done | sort -t: -k2 -rn | head -20
```

### Find historical context comments

```bash
grep -r "was\|because\|historically\|previously\|used to\|before" crates \
  --include="*.rs" | grep -E "^\s*//" | head -20
```

---

## Documentation Debt Matrix

| Crate | Rustdoc | Comments | ADRs Needed | Diagrams | Status |
|-------|---------|----------|-------------|----------|--------|
| andromeda-wal | ⚠️ Partial | 🟡 Adequate | ✅ 1 needed | ❌ None | **P0** |
| andromeda-transaction | ⚠️ Partial | 🟡 Adequate | ✅ 2 needed | ✅ SM needed | **P0** |
| andromeda-tx | ⚠️ Partial | 🟡 Adequate | ✅ 1 needed | ✅ 2PC diagram | **P0** |
| andromeda-quic | ⚠️ Partial | 🟡 Adequate | ✅ 1 needed | ❌ None | **P0** |
| andromeda-catalog | ⚠️ Partial | 🟡 Adequate | ✅ 1 needed | ✅ Batch diagram | **P0** |
| andromeda-recovery | ⚠️ Partial | 🟡 Over-detailed | ✅ 1 needed | ✅ Recovery plan | **P0** |
| andromeda-security | ⚠️ Minimal | 🟡 Adequate | ✅ 1 needed | ❌ None | **P1** |
| andromeda-storage | ⚠️ Minimal | 🔴 Over-detailed | ✅ 3 needed | ❌ None | **P1** |

---

## Next Steps

1. **Weeks 1-2**: Complete C5 rustdoc additions
2. **Weeks 3-4**: Extract decision contexts to ADRs
3. **Weeks 5-6**: Trim over-commented code
4. **Weeks 7+**: Update docs after restructuring

## References

- Rust API Guidelines: https://rust-api-guidelines.rs/
- Andromeda Rust Instructions: `.github/instructions/rust.instructions.md`
- Architecture: `docs/codex/architecture.md`
- ADR Template: `docs/adr/00-TEMPLATE.md` (if exists)
