# DEC-026: Release Gates, Blocker Taxonomy, and V0.5 Deferral Policy

**Status:** Accepted (G4 — Release Gates)  
**Date:** 2025 (Andromeda V0.5)  
**Authors:** Release Governance Agent  
**Impact:** Release approval process, blocker classification, deferral rationale  

---

## Problem Statement

Andromeda V0.5 is a vertical slice that must establish clear, measurable release gates for:

1. **Protocol immutability** — Wire format must be frozen
2. **Storage durability** — Recovery must be crash-safe
3. **HA/DR readiness** — Quorum and backup must be functional
4. **Observability** — Audit trails must be complete and durable
5. **Execution stability** — Plan caching and catalog must be stable
6. **Architecture coherence** — Decisions must be documented; risks tracked

However, there is currently no unified gate taxonomy, no explicit blocker classification system, and no clear deferral policy. This creates ambiguity about what "release ready" means and which issues block release vs. which are post-V0.5.

**Key Challenge:** Distinguish between:
- **Release blockers** — Must be fixed before V0.5 ships
- **Known limitations** — Safe deferrals with documented rationale
- **Post-V0.5 features** — Explicitly out of scope; acceptable deferrals

---

## Design Principles

### 1. Gates Are Measurable

Every gate MUST have:
- A specific criterion (not vague)
- A completion artifact (where evidence is recorded)
- An owner (who verifies)
- A status (PASS | BLOCKED | DEFERRED)

### 2. Blockers Are Explicit

No silent blockers. All release-blocking issues must:
- Be named and documented
- Have a root cause analysis
- Have mitigation path (fix or deferral with rationale)
- Be reviewed by governance

### 3. Deferrals Have Rationale

Post-V0.5 deferrals must explain:
- Why it's safe to defer (vertical slice is complete without it)
- When it will be implemented (phase/timeline)
- Who owns the deferred work

### 4. Vertical Slice Completeness

V0.5 must be a **complete minimal slice**, not a partial implementation:
- Protocol is immutable ✓
- Storage is crash-safe ✓
- HA/DR is quorum-ready ✓
- Observability is auditable ✓
- Execution is plan-cached ✓
- Architecture is governed ✓

Nothing is missing for V0.5 scope; everything else is deferred by design.

---

## Design: Gate Taxonomy

### Gate Categories (6)

#### 1. Protocol Gates (4 gates)
**Objective:** Ensure wire format immutability for V0.5

- **PROT-001:** Frame header layout immutable (52 bytes, field order locked)
- **PROT-002:** Protobuf schema locked (no breaking changes; reserved fields)
- **PROT-003:** FrameType discriminators locked (no reordering)
- **PROT-004:** PayloadKind mappings fixed (round-trip serialization stable)

**Why these matter for vertical slice:**
- Distributed nodes must wire-compatible with each other
- Protocol drift would cause silent incompatibilities
- Once released, protocol must stay stable for compatibility window

**Invariant enforced:** Frame header layout cannot change; discriminators are constant

#### 2. Storage Gates (4 gates)
**Objective:** Ensure storage durability, corruption boundaries, recovery safety

- **STOR-001:** Crash recovery preserves committed writes (LSN chain complete)
- **STOR-002:** Manifest checksum validation (immutable ColdStore publication)
- **STOR-003:** Page layout stable (no silent corruption)
- **STOR-004:** WAL record codec locked (no encoding drift)

**Why these matter for vertical slice:**
- Recovery from crash is the core durability guarantee
- ColdStore immutability ensures restore safety
- Silent corruption is a no-go violation

**Invariant enforced:** WAL-before-visible-commit; manifest immutable; corruption bounded

#### 3. HA/DR Gates (5 gates)
**Objective:** Establish high availability, disaster recovery, restore readiness

- **HADR-001:** Quorum runtime functional (membership tracked; leader election sound)
- **HADR-002:** Promotion eligibility boundary defined (F3↔F4↔F6 separation)
- **HADR-003:** Stream concurrency & backpressure designed (no deadlock)
- **HADR-004:** Restore planning boundary specified (PITR target selection logic)
- **HADR-005:** Backup physical plan validates artifacts (execution guard rails)

**Why these matter for vertical slice:**
- Quorum is required for distributed decision-making
- Promotion eligibility separates F3 (quorum) from F4 (eligibility) from F6 (orchestration)
- Backpressure prevents unbounded SRPL semantics
- Backup artifact validation ensures restore safety

**Invariant enforced:** Quorum membership is sound; promotion is deterministic; backup is valid

#### 4. Observability Gates (3 gates)
**Objective:** Ensure all critical decisions are traceable and auditable

- **OBS-001:** Security audit trails specified (mTLS identity extraction logged)
- **OBS-002:** Recovery traces emitted (startup mode decision recorded)
- **OBS-003:** Decision traces designed (correlation IDs, causality, no-silent-drop)

**Why these matter for vertical slice:**
- Audit trails are forensic evidence for security and recovery events
- No-silent-drop guarantee means all rejections are observable
- Decision traces enable root cause analysis

**Invariant enforced:** No silent decision; all audit points logged; causality edges present

#### 5. Execution Gates (2 gates)
**Objective:** Ensure plan caching and execution stability

- **EXEC-001:** Plan invalidation policy locked (cache stability guaranteed)
- **EXEC-002:** Catalog WAL format stable (DefinitionBatch contract frozen)

**Why these matter for vertical slice:**
- Plan cache invalidation must be deterministic (E6 spec)
- Catalog WAL record format must not change mid-release
- DefinitionBatch is the mutation contract

**Invariant enforced:** Cache entries have bounded lifetime; WAL format is immutable

#### 6. Architecture Gates (3 gates)
**Objective:** Ensure architectural coherence, risk management, doctrine compliance

- **ARCH-001:** ADR decisions complete (25+ decision records present and reviewed)
- **ARCH-002:** Risk register maintained (open risks tracked and mitigated)
- **ARCH-003:** Doctrine compliance verified (no gRPC, SQL, JSON at runtime)

**Why these matter for vertical slice:**
- ADRs document all durable architectural choices
- Risk register ensures no silent risks
- Doctrine rules are project no-go constraints

**Invariant enforced:** All architecture decisions are documented; risks are explicit; doctrine rules hold

---

## Design: Blocker Scoring

### Scoring Levels

| Status | Meaning | Action | Release Impact |
|--------|---------|--------|-----------------|
| **✓ PASS** | Gate criterion met; verification complete | None | Proceed |
| **⚠ BLOCKED** | Gate criterion unmet; issue must be resolved | Fix or decide to defer | Blocks release unless deferred |
| **△ DEFERRED** | Criterion deferred by design; explicitly documented | Scheduled for post-V0.5 | No impact if rationale is sound |

### Blocker Classification

All blockers fall into one of three categories:

#### Category A: Design-Complete, Implementation-Incomplete

**Example:** C4 (admission events)

- Design is sound; specification complete
- Test API incomplete (secondary framework issue)
- Production path unaffected
- **Resolution:** Defer to Wave 8 post-release; design carries forward

#### Category B: Design-Complete, Test-Incomplete

**Example:** E4 (catalog WAL)

- Contract is frozen; codec works
- Test framework has gaps
- Linked to another blocker (E7)
- **Resolution:** Defer to post-release when E7 resolved

#### Category C: Known-Limitation, Acceptable-Scope-Out

**Example:** F5 (restore deferred)

- Backup execution complete
- Restore orchestration deferred to F6+
- Design for restore documented in F5
- **Resolution:** V0.5 scope is backup; restore is F6+ phase

#### Category D: Test-Fixture, Quick-Fix

**Example:** E7 (hash mismatch)

- Root cause is test fixture serialization
- Quick inline fix (1–2 days)
- Either fix inline or defer with sign-off
- **Resolution:** Attempt inline fix; if not feasible, defer with documented risk

#### Category E: Final-Verification, Expected-Pass

**Example:** TBD (final doctrine scan)

- All interim checks passed
- Final comprehensive scan not yet executed
- No violations expected
- **Resolution:** Execute scan; confirm pass; proceed

---

## Design: Deferral Rationale

### When Deferral Is Safe

A feature/task can be deferred from V0.5 if ALL of the following hold:

1. **V0.5 vertical slice remains complete without it**
   - Core protocol immutability ✓
   - Core storage durability ✓
   - Core HA/DR readiness ✓
   - Core observability ✓
   - Core execution stability ✓
   - Core architecture coherence ✓

2. **Deferral does NOT create silent risk**
   - Risk is explicitly documented
   - Deferral has owner and timeline
   - Post-V0.5 phase is identified

3. **Production path is unaffected**
   - Production codec/contract works
   - Production audit trail works
   - Production recovery works

4. **Test/secondary-path gaps are acceptable**
   - Test framework incomplete but not production
   - Can be enhanced post-V0.5
   - Does not block V0.5 functionality

### 7 Deferrals (Documented in BLOCKERS_AND_DEFERRALS.md)

| Feature | Phase | Reason |
|---------|-------|--------|
| GPU analytics | Phase 6 | Not in critical path; V0.5 uses CPU baseline |
| Advanced optimizer | Phase 6 | V0.5 uses baseline; advanced deferred |
| Admin surface | Phase 5 | V0.5 uses mTLS + Procedure API; full admin deferred |
| Multi-procedure | Phase 2+ | V0.5 uses inventory example; generalization deferred |
| Catalog GC | Phase 7+ | V0.5 append-only; GC deferred |
| Performance tuning | Phase 7+ | V0.5 baseline tuning; optimization deferred |
| Admin API/CLI | Phase 5+ | V0.5 programmatic only; admin CLI deferred |

**Deferral Safety:** All 7 are safe deferrals; V0.5 slice is complete without them.

---

## Design: Pre-Release Workflow

### Gate Verification Steps

1. **Define gate (done in G4)** — Specify criterion, owner, completion artifact
2. **Verify gate (done per task)** — Execute tests, collect evidence, document status
3. **Review gate (done before release)** — Release governance reviews status
4. **Sign-off gate (done at release)** — Owner signs off on gate status

### Blocker Triage Steps

When a gate status is **BLOCKED**:

1. **Classify blocker** — Which of 5 categories? (A, B, C, D, E)
2. **Analyze root cause** — Why is criterion unmet?
3. **Choose resolution** — Fix inline or defer? (with risk assessment)
4. **Document decision** — In DEC-026 and BLOCKERS_AND_DEFERRALS.md
5. **Get sign-off** — Owner, governance, and release authority approve
6. **Proceed** — Fix or deferral, release can proceed

### Release Decision Tree

```
All 21 gates defined?
├─ YES → All gates PASS or DEFERRED?
│  ├─ YES → All DEFERRED have documented rationale?
│  │  ├─ YES → No silent blockers?
│  │  │  ├─ YES → All owners have signed off?
│  │  │  │  ├─ YES → ✓ APPROVED FOR RELEASE
│  │  │  │  └─ NO → ⚠ Get remaining sign-offs
│  │  │  └─ NO → ⚠ Escalate to doctrine-guardian (silent blocker)
│  │  └─ NO → ⚠ Document deferral rationale in DEC-026
│  └─ NO → ⚠ Resolve BLOCKED gates (fix or defer)
└─ NO → ⚠ Define missing gates
```

---

## Integration with Andromeda Invariants

### How Gates Protect Project Invariants

| Gate Category | Project Invariant | Protection |
|---------------|-------------------|-----------|
| Protocol | Procedure execution model | Frame header immutability prevents silent wire incompatibilities |
| Storage | WAL-before-visible-commit | Crash recovery preservation ensures durability |
| Storage | Manifest immutability | ColdStore publication freezes restore state |
| HA/DR | Quorum safety | Promotion eligibility prevents split-brain |
| HA/DR | No unbounded SRPL | Backpressure bounds stream concurrency |
| Observability | No-silent-drop | Audit trail completeness ensures forensic trail |
| Execution | Bounded plan cache | Invalidation policy prevents stale plans |
| Architecture | Explicit governance | ADR decisions document all durable choices |

---

## Policy: Post-V0.5 Release Process

### Semantic Versioning

Andromeda uses semantic versioning: **MAJOR.MINOR.PATCH**

- **V0.5.0** — First minor release (V0.x)
- **V0.5.1+** — Patch releases (bug fixes; no new features)
- **V0.6.0+** — Next minor release (new features after Phase 2)
- **V1.0.0** — When all critical gates are locked and stable

### Deprecation Policy

Features deferred from V0.5 are:
- **Pre-announced** in V0.5 release notes (timeline, owners)
- **Scheduled** in Phase 2+ planning (documented in deferral tracker)
- **Implemented** in specified phase (no indefinite deferral)

If a deferred feature is dropped (not implemented post-V0.5), it must:
- Be formally decided in a decision record
- Be announced in release notes
- Have migration plan for users who depend on it

### Backward Compatibility

V0.5 gates ensure:
- **Wire protocol stable** — V0.5 nodes will be compatible with future versions (within compatibility window)
- **Storage format stable** — V0.5 backups will be restorable by future versions
- **Audit trail stable** — V0.5 audit events will be interpretable by future versions

No breaking changes to these contracts without major version bump and long deprecation window.

---

## Policy: Escalation to Doctrine-Guardian

Release gates MUST escalate to `doctrine-guardian` if:

1. **SQL ad hoc detected** — Ad hoc SQL slips into codebase
2. **gRPC surface introduced** — gRPC service definitions appear
3. **Unsafe runtime behavior** — Panics in critical paths; undefined behavior
4. **Unbounded SRPL semantics** — Backpressure mechanism breaks
5. **Unclear recovery implications** — Recovery logic has unanalyzed failure modes

When escalation occurs:
- **Immediate stop** — Release is blocked pending doctrine review
- **Root cause analysis** — Doctrine guardian investigates
- **Decision record** — Captured in ADR with risk assessment
- **Approval or rejection** — Doctrine guardian decides gate status

---

## Decision: Release Gate Framework Accepted

**We ACCEPT the following:**

1. ✓ **Gate taxonomy** — 6 categories, 21 gates defined and measurable
2. ✓ **Blocker scoring** — 3 levels (PASS, BLOCKED, DEFERRED) with classification
3. ✓ **Deferral policy** — 7 deferrals explicitly documented; V0.5 remains complete
4. ✓ **Pre-release workflow** — Gate verification, blocker triage, sign-off process
5. ✓ **Semantic versioning** — V0.5 → V0.6 → V1.0 path; backward compatibility rules
6. ✓ **Escalation policy** — Doctrine violations escalate immediately

**Release Status:** ✓ **READY FOR RELEASE WITH DOCUMENTED DEFERRALS**

(Contingent on E7 resolution and final doctrine scan)

---

## Files Delivered

- **`docs/RELEASE_GATES_V0_5.md`** — 21-gate matrix; gate definitions
- **`docs/READINESS_CHECKLIST.md`** — Binary verification tasks; actionable checklist
- **`docs/BLOCKERS_AND_DEFERRALS.md`** — 5 blocker analysis; 7 deferral rationale
- **`docs/SIGN_OFF_V0_5.md`** — Governance sign-off template; approval record
- **`docs/decisions/DEC-026-release-gates-v0-5.md`** — This decision record

---

## Acceptance Criteria

- [x] Gate taxonomy covers all 44 todos (at least by reference)
- [x] All 5 pre-Wave 8 blockers explicitly listed and dispositioned
- [x] Readiness checklist is actionable (binary checks, not prose)
- [x] Sign-off template is governance-ready (signature lines, checkboxes)
- [x] DEC-026 explains gate philosophy and taxonomy
- [x] Decision relates gate framework to Andromeda invariants
- [x] Deferral policy is explicit and safe
- [x] Escalation rules documented (when to call doctrine-guardian)

---

## Sign-Off

**Decision Record Approved By:**

- [ ] **Release Governance Agent:** _________________________ Date: _________
- [ ] **Project Chief Architect:** _________________________ Date: _________

**Implementation Owner:** Release Governance Agent (G4)

**Related Documents:**
- `docs/RELEASE_GATES_V0_5.md` (gate matrix)
- `docs/READINESS_CHECKLIST.md` (verification tasks)
- `docs/BLOCKERS_AND_DEFERRALS.md` (blocker analysis)
- `docs/SIGN_OFF_V0_5.md` (sign-off template)
- `instructions/QUALITY_GATES.md` (baseline verification)

**Next Steps:**
1. Release governance verifies all gates
2. E7 hash mismatch resolved (inline or deferred)
3. Final doctrine scan executed
4. All owners sign-off on DEC-026
5. SIGN_OFF_V0_5.md completed by release authority
6. Tag v0.5.0
7. Phase 2 begins (post-release blocker resolution)

---

**Document Version:** 1.0  
**Status:** Accepted (G4 — Release Gates)  
**Date:** 2025  
**Last Updated:** [Timestamp at sign-off]
