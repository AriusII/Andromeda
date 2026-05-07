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
- Execution uses validated plan-key identity scaffolding; runtime plan cache remains deferred per DEC-016 ✓
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
**Objective:** Ensure plan-key identity, catalog WAL, and execution stability without enabling a V0.5 runtime plan cache

- **EXEC-001:** Plan-cache identity and future invalidation inputs locked (runtime cache deferred by DEC-016)
- **EXEC-002:** Catalog WAL format stable (DefinitionBatch contract frozen)

**Why these matter for vertical slice:**
- Plan-cache key identity and future invalidation inputs must be deterministic (DEC-016/E6 spec)
- Catalog WAL record format must not change mid-release
- DefinitionBatch is the mutation contract

**Invariant enforced:** No V0.5 runtime plan cache exists; any future cache must be bounded by DEC-016 identity inputs; WAL format is immutable

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
- **Resolution:** Defer to post-release compiler milestone post-release; design carries forward

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
| Execution | Plan-cache scaffold only | DEC-016 defers runtime caching while locking key identity and future invalidation inputs |
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

## V1.0 Release Gate Map Addendum

This addendum extends the accepted V0.5 gate framework to the V1.0 release gate map. V1.0 readiness is not a
follow-on implementation authorization; it is a release-governance consolidation point for blocker disposition,
validation evidence, doctrine compliance, storage-format promotion, protocol/schema governance, recovery safety,
audit/security completeness, benchmark evidence, and explicit deferrals.

### V1.0 gate matrix

| Gate | Required evidence | Blocks V1.0 when | Deferral rule |
|------|-------------------|------------------|---------------|
| Workspace build/test/format | `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo test --workspace`; run `cargo clippy --workspace --all-targets` when the toolchain is available. | Any mandatory command fails, is skipped without release-governance approval, or has unclassified failures. | Documentation-only changes may record commands as not run, but V1.0 release readiness cannot be approved until the full workspace command set is green. |
| Doctrine no-go scan | Classified scans for gRPC, ad hoc SQL/native SQL surface, runtime JSON default, GPU critical-path drift, unsafe drift, and unbounded SRPL semantics. | A scan finds an unapproved SQL surface, gRPC service, runtime JSON default, GPU durability-path dependency, unsafe critical path, or unbounded SRPL behavior. | No doctrine no-go violation is deferrable. Escalate to doctrine-guardian. |
| Storage format promotion | DEC-032 B-Tree key and heap page gates: format identity, golden vectors, canonical heap reader/writer, header/trailer validation, manifest/page/index fingerprints, incompatible artifact rebuild/reject behavior. | Candidate B-Tree or heap bytes are treated as V1 durable compatibility without DEC-032 gates passing. | Prototype/non-release durable artifacts may remain explicitly non-release. V1.0 durable promotion must not be deferred silently. |
| Protobuf and QUIC schema governance | DEC-021/DEC-022b compatibility evidence: protocol version lock, descriptor hash stability, reserved fields/values where changed, metadata-before-payload, result completion semantics, no gRPC service surface, frame/payload discriminator lockstep. | Breaking schema/frame drift is accepted without a decision record, compatibility note, and rejection behavior. | Additive schema work may defer only if it is unreachable in V1.0 and marked reserved or disabled by contract. |
| WAL, recovery, and crash safety | WAL-before-visible-commit evidence; committed-only replay; incomplete transaction handling; corruption-boundary tests; reproducible crash seeds/scenarios; storage-format validation before redo. | Recovery diverges, replays past corruption, publishes incomplete state, lacks deterministic crash evidence, or cannot reject unknown storage formats before mutation. | No recovery safety defect is deferrable for V1.0. |
| Audit and security | Authorization-before-transaction evidence; least-privilege review; audit events for contract rejection, authorization rejection, WAL append/flush, commit visibility, rollback, recovery, catalog mutation, protocol rejection, and corruption boundary; secret-scan disposition. | Security bypass, post-transaction authorization, missing critical audit evidence, or secret exposure is found. | Non-critical observability enrichment can defer only with owner, timeline, and no loss of forensic evidence. |
| Benchmark and performance budget | Bounded benchmark scenarios with workload assumptions, hardware profile, versioned statistics, and regression comparison against the last accepted baseline. | A critical-path benchmark regresses by more than 10% in latency or throughput, or any p95/p99/storage/recovery budget is missing for a claimed V1.0 performance readiness statement. | Optimization may defer; unexplained critical-path regression may not. Predictive evidence cannot approve release alone. |
| Known blocker and deferral register | Every blocker has status `PASS`, `BLOCKED`, or `DEFERRED`, owner, rationale, mitigation, and target phase; risk register is updated for residual release risk. | A blocker is unnamed, ownerless, missing mitigation, or deferred without decision-record rationale. | Deferral is allowed only when the production path remains recoverable, secure, observable, contract-compatible, and explicitly out of V1.0 scope. |

### V1.0 explicit blockers

The following conditions are release-blocking until fixed or explicitly dispositioned by a decision record that does not
violate doctrine:

1. Any release blocker listed in `.agents/instructions/QUALITY_GATES.md`.
2. Any DEC-032 storage-format gate that remains unmet while claiming V1.0 durable format compatibility.
3. Any Protobuf/QUIC compatibility drift that lacks reserved-field/value governance or fail-fast rejection behavior.
4. Any WAL/recovery/crash gap that can publish incomplete, corrupt, unauthorized, or unreproducible state.
5. Any missing audit event for a critical rejection, mutation, visibility, recovery, or corruption boundary.
6. Any benchmark regression above the V1.0 threshold without an approved mitigation or scope reduction.
7. Any silent or ownerless deferral.

### V1.0 benchmark threshold

For V1.0 release readiness, a benchmark gate is considered failed when a critical-path workload regresses by more than
10% against the most recent accepted baseline for latency, throughput, recovery time, WAL append/flush behavior, or
storage publication behavior. The benchmark evidence must identify workload assumptions, hardware profile, baseline
version, current version, confidence/freshness/stability considerations, and whether the benchmark is advisory or
release-blocking. Predictive or statistical evidence may inform release risk but must not decide release readiness alone.

### V1.0 deferral rule

A V1.0 deferral is safe only when all of the following are true:

1. The V1.0 production path remains recoverable, secure, observable, and contract-compatible without the deferred work.
2. The deferred item does not mask a doctrine no-go, recovery uncertainty, storage-format drift, or audit omission.
3. The deferral has an owner, target phase, mitigation, rollback or disablement note, and risk-register linkage.
4. Release notes or handoff material state the limitation without implying V1.0 support.

---

## Files Delivered

- Legacy V0.5 release-gate matrix — 21-gate matrix and gate definitions.
- Legacy V0.5 readiness checklist — Binary verification tasks and actionable checklist.
- Legacy V0.5 blocker register — 5 blocker analysis and 7 deferral rationales.
- Legacy V0.5 sign-off template — Governance sign-off template and approval record.
- **`documentations/governance/decisions/DEC-026-release-gates-and-deferral-policy.md`** — This decision record

---

## Acceptance Criteria

- [x] Gate taxonomy covers all 44 todos (at least by reference)
- [x] All 5 pre-post-release compiler milestone blockers explicitly listed and dispositioned
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
- Legacy V0.5 gate matrix.
- Legacy V0.5 verification checklist.
- Legacy V0.5 blocker analysis.
- Legacy V0.5 sign-off template.
- `.agents/instructions/QUALITY_GATES.md` (baseline verification)

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
