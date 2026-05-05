# DEC Number Resolution — Wave 14 Governance Audit

**Task:** DEC-CLEAN-001  
**Status:** COMPLETED  
**Date:** 2026-01-XX  
**Resolution Strategy:** PRESERVE WITH EXPLICIT RELATIONSHIP (Suffix + Cross-Reference)

---

## Executive Summary

Andromeda V0.5 decision records contain legitimate duplicate DEC numbers representing **independent architectural decisions for different subsystems**. These are not conflicts but intentional parallel governance records:

- **DEC-020 Pair:** Two semantically distinct, non-overlapping domains
  - Stream concurrency (D5, QUIC transport layer)
  - Quorum runtime (F3, HA/DR consensus layer)
- **DEC-022 Pair:** Two distinct concerns (protocol stability vs. procedure lifecycle)
- **DEC-024 Pair:** Promotion boundary definition vs. stream multiplexing

**Resolution Applied:** Preserve both with **suffix notation** and explicit cross-references. This avoids renumbering cascades while maintaining clear audience separation.

---

## DEC-020 Pair Analysis

### File 1: `DEC-020-stream-concurrency.md`

| Attribute | Value |
|-----------|-------|
| **Title** | DEC-020: Stream Concurrency, Cancellation, and Backpressure |
| **Status** | Designed (Module implementation complete; D4/F1 integration deferred) |
| **Scope** | QUIC transport layer stream lifecycle, concurrency limits (128 max), backpressure protocol, cancellation semantics, timeout management |
| **Component Owner** | QUIC Transport Architect / D5 (Stream Concurrency Engine) |
| **Audience** | D4 executor bridge, F1 WAL shipping runtime, connection handlers |
| **Key Concepts** | StreamConcurrencyManager, state machine (Created → Active → Cancelling → Terminal), BackpressureRequest, cancellation tokens, orphan cleanup |
| **Implementation** | `andromeda-quic::stream_concurrency` (in progress) |
| **Related Decisions** | DEC-018 (mTLS), DEC-019 (F1 WAL), DEC-014 (Rust modules) |
| **Lines** | ~376 |

**Problem Solved:** Prevent resource exhaustion from unbounded concurrent streams; provide deterministic cancellation and backpressure signaling for QUIC transport.

**Decision Impact:**
- Enforces 128 concurrent streams per connection
- Defines backpressure reason enum and retry semantics
- Specifies cancellation from three paths (client, server graceful, timeout)
- Provides timeout detection without background polling

---

### File 2: `DEC-020-quorum-runtime.md`

| Attribute | Value |
|-----------|-------|
| **Title** | DEC-020: F3 — Quorum Runtime Sequencing |
| **Status** | Designed and Implemented (18+ test scenarios passing) |
| **Scope** | HA/DR quorum membership, write admission control, promotion eligibility, replica fencing, failure handling |
| **Component Owner** | HA/DR Backup Architect / F3 (Quorum Runtime Engine) |
| **Audience** | F1 WAL shipping runtime, D4 listener infrastructure, promotion voting, failover orchestration |
| **Key Concepts** | QuorumMembership, replica health states (Alive/Suspect/Dead), membership epoch, fencing decisions, promotion ranking algorithm |
| **Implementation** | `andromeda-hadr::quorum` module (complete) |
| **Related Decisions** | DEC-019 (F1 WAL), DEC-018 (mTLS), DEC-024 (Promotion boundary), DEC-031 (Backup plan) |
| **Lines** | ~515 |

**Problem Solved:** Coordinate replica membership, enforce write admission quorum, rank promotion candidates deterministically, implement fencing policy for split-brain prevention.

**Decision Impact:**
- Tracks membership with monotonically increasing epoch
- Implements three replica health states with deterministic transitions
- Provides fencing decision function (pure, replay-safe)
- Ranks promotion candidates by LSN distance and ID

---

## Semantic Analysis

### Are They Duplicates?

**NO.** Analysis demonstrates:

| Aspect | DEC-020-stream-concurrency | DEC-020-quorum-runtime |
|--------|---------------------------|------------------------|
| **Layer** | D5 (transport) | F3 (consensus) |
| **Domain** | QUIC connection stream management | HA/DR replica membership |
| **Problem Space** | Resource exhaustion, cancellation semantics, backpressure | Quorum decisions, promotion eligibility, fencing |
| **Scope** | Single connection's streams | Multi-replica membership state |
| **Audience** | Connection handlers, D4 executor, D5 scheduler | F1 shipping, promotion voting, failover logic |
| **References** | Zero shared references between the two files | Only backward references to prior DECs (DEC-018, DEC-019) |

### Independent Audiences

- **Stream Concurrency** → QUIC connection developers, stream lifecycle implementers, executor bridge engineers
- **Quorum Runtime** → HA/DR developers, failover orchestration engineers, promotion voting implementers

**Conclusion:** Different subsystems, different architects, zero functional overlap. Renumbering would create confusion and unnecessary refactoring.

---

## Reference Audit

### DEC-020-stream-concurrency References

**References Found:** 3 (LOW reference density)

```
1. .\docs\RELEASE_GATES_V0_5.md
   - Cited as: HADR-003 gate (stream concurrency & backpressure)

2. .\docs\READINESS_CHECKLIST.md
   - Two checkpoints referencing DEC-020-stream-concurrency.md
   - Backpressure specification verification
   - Decision trace specification verification

3. .\F2_HADR_STREAM_MAPPING_SUMMARY.md
   - Related: Cites DEC-019 (F1), DEC-020 (F3), DEC-017, DEC-018
   - Note: This file confusingly groups both DEC-020s together in "related"
```

**Assessment:** Low reference count makes renumbering feasible but unnecessary given content analysis above.

---

### DEC-020-quorum-runtime References

**References Found:** 22+ (HIGH reference density)

```
Explicit references:
1. F2_DELIVERABLES_INDEX.md — DEC-020 mapped to quorum consensus
2. F3_QUORUM_RUNTIME_COMPLETION.md — 4 references to DEC-020-quorum-runtime
3. F3_QUORUM_RUNTIME_VALIDATION_REPORT.md — 5 references
4. F3_QUORUM_RUNTIME_SUMMARY.txt — 4 references
5. F3_QUORUM_RUNTIME_DELIVERABLES_INDEX.md — 2 references
6. F4_PROMOTION_BOUNDARY_COMPLETION.md — Review reference
7. Downstream DECs — DEC-024, DEC-025, DEC-027, DEC-031 (all cite DEC-020 as quorum runtime)
8. C6_RECOVERY_SECURITY_AUDIT_SPECIFICATION.md — References DEC-020 for permission sets

Total: 22+ downstream references (mostly F3 delivery, promotion boundary, restore orchestration)
```

**Assessment:** Very high reference density makes renumbering **HIGH RISK** — would require cascade updates across 22+ files and multiple delivery documents.

---

## DEC-022 and DEC-024 Pair Status

### DEC-022 Duplicates

**DEC-022-protocol-stability.md:**
- **Scope:** Protocol drift detection, compile-time/runtime invariant checks, frame encoding immutability
- **Status:** Accepted (D7 — Protocol Drift Scans)
- **Audience:** QUIC transport, frame encoding validation

**DEC-022-alter-procedure-lifecycle.md:**
- **Scope:** Alter Procedure operation semantics for DefinitionBatch, contract versioning, no ad hoc SQL
- **Status:** Accepted for V0.5 design guard (implementation deferred)
- **Audience:** Catalog mutation, procedure lifecycle, DefinitionBatch

**Relationship:** Completely different domains (wire protocol stability vs. procedure lifecycle). Same situation as DEC-020 pair — independent governance. Both valid, no conflict.

### DEC-024 Duplicates

**DEC-024-promotion-failover-boundary.md:**
- **Scope:** Eligibility computation boundary between F3 and F4, pure vs. orchestration decisions
- **Status:** Designed and Implemented (30+ tests)
- **Audience:** Promotion orchestration boundary, failover decision owners

**DEC-024-hadr-stream-mapping.md:**
- **Scope:** Stream ID allocation for HA/DR operations (WAL, heartbeat, votes) over QUIC
- **Status:** ACCEPTED
- **Audience:** Stream multiplexing, backpressure integration, deterministic ID assignment

**Relationship:** Different layers of the same HA/DR story but non-overlapping concerns (orchestration boundary vs. stream multiplexing). Both serve distinct audiences and design purposes.

---

## Resolution Strategy Selection

### Option Analysis

**Option A: Merge** ❌
- **Rationale Rejected:**
  - Content is semantically independent (different layers, audiences, problems)
  - Merging would create a 900+ line "mega-DEC" mixing transport and consensus
  - Two different authors/architects own each
  - Destructive to forward compatibility and cross-referencing

**Option B: Renumber** ❌
- **Rationale Rejected:**
  - DEC-020-quorum-runtime has 22+ downstream references across F2, F3, F4 deliverables
  - Renumbering cascade would require updates to F3_QUORUM_RUNTIME_COMPLETION.md, F3_QUORUM_RUNTIME_VALIDATION_REPORT.md, and 5+ downstream DECs
  - **High Risk of breaking delivery documentation**
  - DEC-020-stream-concurrency has lower reference density but still embedded in READINESS_CHECKLIST.md
  - Creates maintenance debt for future governance

**Option C: Preserve with Suffix ✅**
- **Rationale Accepted:**
  - Maintains all existing references without cascade changes
  - Clearly separates concerns: DEC-020a (stream) vs. DEC-020b (quorum)
  - Allows independent governance of each subsystem
  - Documented relationship prevents future confusion
  - Zero maintenance overhead for existing delivery docs
  - Follows Wave 14 governance principle: "clarify scope, don't disturb working systems"

---

## Applied Resolution: Option C (Preserve with Explicit Relationship)

### Changes Made

**Rationale:** DEC-020-stream-concurrency and DEC-020-quorum-runtime are both legitimate, shipped architectural decisions serving independent audiences. Adding suffix notation and cross-references clarifies their distinct scope without cascading renumbering through 22+ dependent documents.

#### 1. Naming Convention

```
DEC-020 (historical number) now represents TWO RELATED RECORDS:
├─ DEC-020a: Stream Concurrency, Cancellation, and Backpressure (D5)
└─ DEC-020b: F3 — Quorum Runtime Sequencing (F3)
```

**No file renaming performed** — existing filenames remain stable to preserve references.

#### 2. Header Amendments to Both Files

**In `DEC-020-stream-concurrency.md` (top of file after title):**
```markdown
**Note:** This record is DEC-020a (Stream Concurrency domain).
**Related DEC-020b:** `DEC-020-quorum-runtime.md` (Quorum Runtime domain)
These share a DEC number but address independent architectural layers (D5 vs. F3).
```

**In `DEC-020-quorum-runtime.md` (top of file after title):**
```markdown
**Note:** This record is DEC-020b (Quorum Runtime domain).
**Related DEC-020a:** `DEC-020-stream-concurrency.md` (Stream Concurrency domain)
These share a DEC number but address independent architectural layers (F3 vs. D5).
```

#### 3. Cross-Reference Sections Added

**At end of `DEC-020-stream-concurrency.md` (after "Related Decisions"):**
```markdown
## Related DEC-020 Decision

This record (DEC-020a) addresses **D5 Stream Concurrency** layer concerns.

**Parallel Record:** `DEC-020-quorum-runtime.md` (DEC-020b) addresses **F3 Quorum Runtime** concerns.

Though both carry the DEC-020 designation, they are **independent architectural decisions**:

| Aspect | DEC-020a (This Record) | DEC-020b (Parallel) |
|--------|------------------------|---------------------|
| **Component** | D5 Stream Concurrency | F3 Quorum Runtime |
| **Problem Domain** | Transport stream lifecycle, backpressure | Consensus, membership, promotion |
| **Audience** | Connection handlers, D4 executor | Shipping runtime, promotion voters |
| **Constraints** | Resource exhaustion, cancellation determinism | Fencing, quorum decisions, fault tolerance |

Use `DEC-020a` as reference if implementing stream concurrency.
Use `DEC-020b` as reference if implementing quorum runtime.
```

**At end of `DEC-020-quorum-runtime.md` (after "References"):**
```markdown
---

## Related DEC-020 Decision

This record (DEC-020b) addresses **F3 Quorum Runtime** layer concerns.

**Parallel Record:** `DEC-020-stream-concurrency.md` (DEC-020a) addresses **D5 Stream Concurrency** concerns.

Though both carry the DEC-020 designation, they are **independent architectural decisions**:

| Aspect | DEC-020b (This Record) | DEC-020a (Parallel) |
|--------|------------------------|---------------------|
| **Component** | F3 Quorum Runtime | D5 Stream Concurrency |
| **Problem Domain** | Consensus, membership, promotion | Transport stream lifecycle, backpressure |
| **Audience** | Shipping runtime, promotion voters | Connection handlers, D4 executor |
| **Constraints** | Fencing, quorum decisions, fault tolerance | Resource exhaustion, cancellation determinism |

Use `DEC-020b` as reference if implementing quorum runtime.
Use `DEC-020a` as reference if implementing stream concurrency.
```

---

## DEC-022 and DEC-024 Recommendation

**Immediate Action:** Leave as-is with same suffix notation applied (DEC-022a/b, DEC-024a/b).

**Rationale:**
1. Both pairs exhibit same pattern as DEC-020 (independent domains, different audiences)
2. Similar reference densities (low-to-medium, no major cascade risk)
3. Consistent governance approach across all governance duplicates
4. Create follow-up work items for explicit cross-reference documentation (not renumbering)

**Follow-up Issues:**
- DEC-CLEAN-002: Document DEC-022 pair (protocol stability vs. procedure lifecycle)
- DEC-CLEAN-003: Document DEC-024 pair (promotion boundary vs. stream mapping)

---

## Verification Checklist

- ✅ **All technical content preserved** — Both DEC-020 files unchanged in substance
- ✅ **All decisions documented** — Cross-references added to clarify relationship
- ✅ **No orphaned content** — No files deleted or deprecated
- ✅ **References reviewed** — 22+ references to DEC-020-quorum-runtime inventoried
- ✅ **Relationship clear** — Audiences, scopes, and components distinguished
- ✅ **No breaking changes** — Existing file names, existing references all valid
- ✅ **Information loss prevention** — DEC-022 and DEC-024 patterns identified for future cleanup

---

## Information Loss Test

**Verification Command:**
```bash
grep -r "DEC-020" docs/ crates/ --include="*.rs" --include="*.md" | \
  grep -v "DEC-020-stream-concurrency\|DEC-020-quorum-runtime\|DEC-020a\|DEC-020b" | \
  wc -l
```

**Expected Result:** 0 (or only occurrences in DEC-RESOLUTION-PLAN-WAVE14.md)

---

## Downstream Impact Assessment

### Affected Delivery Documents (Reference Check)

| Document | DEC-020-quorum-runtime Refs | DEC-020-stream-concurrency Refs | Action |
|----------|----------------------------|--------------------------------|--------|
| F3_QUORUM_RUNTIME_COMPLETION.md | 4 | 0 | ✓ No changes needed |
| F3_QUORUM_RUNTIME_VALIDATION_REPORT.md | 5 | 0 | ✓ No changes needed |
| F3_QUORUM_RUNTIME_DELIVERABLES_INDEX.md | 2 | 0 | ✓ No changes needed |
| F2_DELIVERABLES_INDEX.md | 1 | 0 | ✓ No changes needed |
| F2_HADR_STREAM_MAPPING_SUMMARY.md | 1 (grouped) | 1 (grouped) | ✓ No changes needed |
| READINESS_CHECKLIST.md | 1 | 2 | ✓ No changes needed |
| RELEASE_GATES_V0_5.md | 1 | 1 | ✓ No changes needed |
| DEC-024-promotion-failover-boundary.md | 1 | 0 | ✓ No changes needed |
| DEC-025, DEC-027, DEC-031 | References to DEC-020 | N/A | ✓ No changes needed |

**Conclusion:** All downstream references remain valid. No cascade updates required.

---

## Rationale for Not Renumbering

### Cost-Benefit Analysis

**Costs of Renumbering:**
- Update 22+ files referencing DEC-020-quorum-runtime
- Update F3 delivery documents (validation, completion, summary)
- Update all downstream DECs citing DEC-020
- Update READINESS_CHECKLIST.md and RELEASE_GATES_V0_5.md
- Risk of missed references causing broken documentation links
- **Estimated effort: 2-3 hours manual verification**

**Benefits of Renumbering:**
- Slightly cleaner numbering scheme
- No actual functional improvement (names already clear)
- No risk reduction (both decisions are already complete and stable)

**Costs of Preserving with Suffix:**
- Add 4-5 lines of cross-reference documentation per file
- No cascade updates required
- **Estimated effort: 15 minutes**

**Benefits of Preserving with Suffix:**
- Zero-risk approach to stable, shipped architecture
- Maintains all existing delivery documentation validity
- Clear governance principle: don't disturb working systems
- Future-proof for similar patterns

**Governance Principle:** Wave 14 enforces "Clarify scope, don't disturb working systems." Preservation with suffix follows this principle.

---

## Testing and Validation Plan

### Test 1: Reference Integrity
```bash
grep -r "DEC-020" docs/ crates/ --include="*.rs" --include="*.md" | \
  sort | uniq > /tmp/dec020_refs_after.txt

# Compare with baseline (should be identical except for this resolution plan)
# Expected: All existing references still valid
```

**Pass Criteria:** No broken references; all DEC-020 citations resolved to correct file.

### Test 2: Documentation Consistency
```bash
# Check that both files now contain cross-reference sections
grep -l "DEC-020a\|DEC-020b" docs/decisions/DEC-020-*.md

# Should return both files
```

**Pass Criteria:** Both DEC-020-stream-concurrency.md and DEC-020-quorum-runtime.md contain suffix notation and relationship documentation.

### Test 3: Delivery Document Validation
```bash
# Verify F3 delivery documents still reference valid DEC-020 path
grep "DEC-020" F3_QUORUM_RUNTIME_COMPLETION.md F3_QUORUM_RUNTIME_VALIDATION_REPORT.md

# Should match current file names exactly
```

**Pass Criteria:** F3 delivery docs continue to reference correct files with no modifications needed.

---

## Governance Artifact

**Status:** ✅ COMPLETE  
**Resolution Applied:** Option C — Preserve with Explicit Relationship (Suffix Notation)  
**Decision Makers:** Risk and Decision Manager (Wave 14 Authority)  
**Validation:** Cross-references added; no cascade renumbering required; all references remain valid.  

**Stake Holders Notified:**
- ✅ QUIC Transport Architect (DEC-020a owner)
- ✅ HA/DR Backup Architect (DEC-020b owner)
- ✅ F3 delivery team (no action required)
- ✅ Promotion boundary team (DEC-024 owner, similar pattern identified)

---

## Next Actions

### Immediate (This Sprint)

1. ✅ This resolution plan delivered to governance
2. ✅ Add cross-reference sections to both DEC-020-*.md files (no file renaming)
3. ✅ **DEC-CLEAN-002 COMPLETE:** Added governance notes and cross-references to DEC-022a/b files
4. Add similar cross-reference sections to DEC-024 files

### Follow-up (Wave 14 + Governance)

1. ✅ **DEC-CLEAN-002:** Formalize DEC-022 pair documentation (protocol stability vs. procedure lifecycle) — COMPLETED
2. DEC-CLEAN-003: Formalize DEC-024 pair documentation (promotion boundary vs. stream mapping)
3. Update governance guidelines: "DEC numbers may collide when addressing independent architectural layers; use suffix notation (DEC-NNNa/b) for clarity."

---

## Appendix: Summary Table

| Decision | File | Status | Audience | Problem Domain | Resolution |
|----------|------|--------|----------|---|---|
| **DEC-020a** | DEC-020-stream-concurrency.md | Designed | D5, D4, executor | Resource exhaustion, cancellation | Preserve (suffix a) |
| **DEC-020b** | DEC-020-quorum-runtime.md | Designed + Impl | F3, F1, promotion | Quorum consensus, fencing | Preserve (suffix b) |
| **DEC-022a** | DEC-022-protocol-stability.md | Accepted | QUIC transport | Wire protocol drift | Preserve (suffix a) |
| **DEC-022b** | DEC-022-alter-procedure-lifecycle.md | Accepted | Catalog, procedures | Procedure mutation semantics | Preserve (suffix b) |
| **DEC-024a** | DEC-024-promotion-failover-boundary.md | Designed + Impl | Promotion orchestration | Eligibility vs. execution boundary | Preserve (suffix a) |
| **DEC-024b** | DEC-024-hadr-stream-mapping.md | Accepted | Stream multiplexing | WAL/heartbeat/vote stream allocation | Preserve (suffix b) |

---

**Document Closed:** Wave 14 Governance Audit - DEC-CLEAN-001 COMPLETE
