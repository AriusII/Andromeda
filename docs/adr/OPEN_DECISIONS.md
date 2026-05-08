# Open Decisions Registry

**Generated**: 2026-05-08  
**Purpose**: Track architectural decisions blocking or affecting Phase 1-7  
**Status**: Phase 0 baseline

## Quick Reference

| Decision | Owner | Status | Blocking | Link |
|----------|-------|--------|----------|------|
| Page size | Storage team | 🔴 **OPEN** | Phase 2+ | #D1 |
| ContractHash canonicalization | Catalog team | 🔴 **OPEN** | Phase 2+ | #D2 |
| CatalogVersion granularity | Catalog team | 🔴 **OPEN** | Phase 2+ | #D3 |
| GPU placement (off commit) | Optimizer team | 🟡 **TENTATIVE** | Phase 3+ | #D4 |
| Optimizer placement (C3 vs C4) | Optimizer team | 🟡 **TENTATIVE** | Phase 2+ | #D5 |
| HADR failover strategy | HA/DR team | 🟡 **TENTATIVE** | Phase 4+ | #D6 |
| Recovery startup modes | Recovery team | 🟢 **DECIDED** | None | #D7 |

---

## 🔴 CRITICAL OPEN DECISIONS

### D1: Page Size

**Context**: Physical page size affects storage layout, I/O patterns, buffer pool efficiency

**Question**: Should Andromeda support 4KB, 8KB, or 16KB pages? Or variable per table?

**Impact**:
- Storage format (breaking change if modified later)
- Buffer pool tuning (cache lines, eviction)
- B-tree node capacity (key/value density)
- Network message sizes (affects RPC framing)

**Decision Needed By**: Phase 2 start (before storage restructuring)

**Placeholder ADR**: None yet (create when deciding)

**Candidates**:
1. Fixed 16KB (like PostgreSQL) - simplicity
2. Fixed 8KB (like MySQL InnoDB) - balance
3. Fixed 4KB (like Linux pages) - compatibility
4. Variable per table (like some NoSQL) - flexibility but complex

**Next Step**: Storage team → create ADR with evidence from benchmarks

---

### D2: ContractHash Canonicalization

**Context**: Procedures have contracts (type signatures, arg/return types). These need stable hashes for versioning, deduplication, compatibility checking.

**Question**: How to canonicalize procedure contracts into stable hash?

**Impact**:
- Procedure versioning (catalog)
- Cross-version compatibility
- SDK generation (hash uniqueness)
- Contract storage (dedupable?)

**Decision Needed By**: Phase 2 (before catalog restructuring)

**Placeholder ADR**: None yet (create when deciding)

**Candidates**:
1. Protobuf canonical form → hash - deterministic, but protouf-specific
2. JSON canonical form → hash - portable, but not human-friendly
3. Custom binary canonicalization - full control, but more code
4. Semantic equivalence classes - complex, deduplicates similar contracts

**Next Step**: Catalog team → survey existing approaches, create ADR

---

### D3: CatalogVersion Granularity

**Context**: Catalog versioning affects DDL safety, DefinitionBatch semantics, recovery procedures.

**Question**: Should versions track:
- (a) Per-object (table version, procedure version separately)?
- (b) Per-batch (all objects in batch have same version)?
- (c) Per-database (global catalog version)?
- (d) Hybrid approach?

**Impact**:
- DefinitionBatch atomicity
- Recovery replay (which objects to restore?)
- Compatibility checking (which procedures can talk to which versions?)
- Plan cache invalidation

**Decision Needed By**: Phase 2 (before catalog restructuring)

**Placeholder ADR**: None yet (create when deciding)

**Candidates**:
1. Per-object - fine-grained, compatible objects coexist
2. Per-batch - transactional, simpler recovery
3. Per-database - global, simple but less flexible
4. Hybrid - tradeoff between simplicity and flexibility

**Next Step**: Catalog team → analyze DefinitionBatch use cases, create ADR

---

### D4: GPU Placement (Off-Commit Policy)

**Context**: GPU can accelerate analytics, statistics, vector operations. Must not participate in commit path.

**Question**: What are allowed GPU use cases?

**Impact**:
- Analytics performance (GPU accelerated aggregates?)
- Statistics collection (GPU histogram building?)
- Vector operations (GPU similarity search?)
- Commitment: GPU code NEVER sees commit/WAL/rollback/MVCC short-visibility

**Decision Needed By**: Phase 3+ (analytics phase)

**Placeholder ADR**: Policy exists (GPU off-commit), but use-case list not finalized

**Candidates**:
1. Statistics only (low risk, limited performance gain)
2. Analytics + statistics (good ROI, medium complexity)
3. Analytics + statistics + vectors (high performance, high complexity)
4. Analytics + statistics + vectors + batch sort (maximum features, maximum complexity)

**Next Step**: GPU team + optimizer team → create ADR with performance evidence

---

### D5: Optimizer Placement (C3 vs C4)

**Context**: Query optimizer is currently C3 (general). Affects plan cache, statistics, cardinality.

**Question**: Should optimizer be C4 (critical) or stay C3 (general)?

**Impact**:
- Testing requirements (C4 → integration + property tests)
- Review rigor (C4 → expert review)
- Release gating (C4 → pre-release validation)
- Performance SLO (C4 → observable, bounded, versioned)

**Decision Needed By**: Phase 2 (before Phase acceptance criteria)

**Placeholder ADR**: None yet

**Candidates**:
1. Keep C3 - optimizer is pluggable, plan fallback exists (current)
2. Promote to C4 - optimizer performance affects user experience
3. Hybrid - optimizer core is C4, strategies are C3

**Next Step**: Optimizer team → performance impact analysis, create ADR

---

### D6: HADR Failover Strategy

**Context**: HA/DR quorum, failover, promotion - affects replica consistency, split-brain prevention, RTO/RPO.

**Question**: Consensus algorithm for quorum?

**Impact**:
- Network partitions (how handled?)
- Promotion eligibility (which replicas can become primary?)
- Split-brain prevention (fencing policy)
- Witness/arbiter requirements

**Decision Needed By**: Phase 4+ (HA/DR restructuring)

**Placeholder ADR**: Decision pending, Raft/Paxos analysis in progress

**Candidates**:
1. Raft consensus - proven, widespread, simpler
2. Paxos variants - more flexible, complex
3. Custom quorum - tailored to Andromeda, but unique
4. Etcd-like - proven in other systems, operational knowledge

**Next Step**: HA/DR team → comparative analysis, create ADR

---

## 🟡 TENTATIVE DECISIONS (Preliminary)

### D7: Recovery Startup Modes ✅

**Status**: DECIDED (documented)

**Modes**:
- **FastStart**: Quick startup, unverified consistency (for development)
- **SafeStart**: Verified startup with consistency checks (default production)
- **ForensicStart**: Detailed logging, crash analysis (incident response)

**ADR**: `docs/adr/ADR-037-recovery-startup-modes.md` (if exists)

**Implementation**: `andromeda-recovery` module

---

### D8: Procedure Versioning (Preliminary)

**Status**: TENTATIVE (needs validation)

**Concept**: Procedures can coexist at multiple versions. New procedures use latest version. Clients specify version on invocation (or default to latest).

**Decision Needed By**: Phase 2 (before catalog implementation)

**Next Step**: Catalog team → prototype multi-version procedure resolution

---

## 🟢 DECIDED (Locked)

### GPU Off-Commit Path Policy

**Decision**: GPU code may never run in:
- ❌ Commit path (tx → wal → commit_log)
- ❌ WAL replay (recovery)
- ❌ Rollback path
- ❌ MVCC short-visibility operations
- ❌ Catalog publication
- ❌ Security audit trail

**Allowed**:
- ✅ Statistics collection (pre-computation)
- ✅ Analytics queries (batch)
- ✅ Vector embeddings (batch)

**Enforcement**: Code review + CI validation

**Reference**: `.github/instructions/gpu-no-commit-policy.md` (or similar)

---

### QUIC-Only RPC Policy

**Decision**: No gRPC. RPC surface uses QUIC only.

**Why**: Custom QUIC implementation allows:
- Zero-copy receive paths
- Custom frame semantics (admission, backpressure)
- Tight integration with WAL shipping

**Enforcement**: No gRPC dependencies allowed in workspace.rs

**Reference**: Custom instruction + code review

---

### Protobuf Schema Only (No Application SQL)

**Decision**: All application interaction is procedure-based. Schema defined in Protobuf, not ad hoc SQL.

**Why**: Enterprise-grade schema management, versioning, codegen.

**Enforcement**: No dynamic SQL generation in public APIs.

**Reference**: Custom instruction + security review

---

## Decision Process

### For Open Decisions

When proposing a decision:

1. **Create GitHub Issue** labeled `architecture/decision`
   - Title: "Decision: [Topic]"
   - Description: Context, impact, candidates

2. **Create ADR Stub** in `docs/adr/`
   - Use template: `00-TEMPLATE.md`
   - Status: "PROPOSED" or "DRAFT"
   - Placeholder for analysis

3. **Gather Evidence**
   - Benchmarks (if performance-related)
   - Survey (if adoption-related)
   - Prototyping (if technical risk)

4. **Present ADR**
   - Get 2+ expert approvals
   - Link to evidence
   - Document rationale

5. **Move to DECIDED**
   - Update ADR status to "DECIDED"
   - Announce in release notes
   - Update this registry

---

## Timeline

### Phase 0 (Current): Identify

- ✅ D1-D3, D5-D6 identified as OPEN
- ✅ D7-D8 identified as TENTATIVE
- ✅ GPU + QUIC + Protobuf locked as DECIDED

### Phase 1 (Weeks 3-4): Defer

- 🔄 D1-D3, D5 → Create ADR stubs, defer decision
- 🔄 D6 → Not needed until Phase 4

### Phase 2 (Weeks 5-6): Decide

- 🔄 D1-D3, D5 → Final decision, move to DECIDED
- 🔄 D8 → Validate prototype

### Phase 3+ (Weeks 7+): Implement

- 🔄 Implementation follows finalized decisions

---

## Open Questions Not Yet on Registry

These are research topics, not blocking decisions:

- [ ] Analytics performance benchmarks (D4 preparation)
- [ ] Cardinality estimation accuracy targets
- [ ] Plan cache hit rate targets
- [ ] RTO/RPO targets for HA/DR (D6 preparation)
- [ ] Fuzz target coverage targets

**Action**: Add to registry once they impact decisions.

---

## Next Steps

1. **Week 1**: Socialize this registry (architecture meeting)
2. **Week 2**: Schedule ADR working sessions for D1-D3, D5, D6
3. **Week 3**: Prototype sessions for D8
4. **Week 4**: Finalize D1, D2, D3 (ADR approval)
5. **Phase 2**: Implement decisions

---

## Tools

### Track ADR Status

```bash
# List all ADRs
ls docs/adr/

# Check decision status
grep -r "Status:" docs/adr/*.md | grep -E "PROPOSED|DECIDED"
```

### Update Registry

```bash
# When ADR moves from PROPOSED to DECIDED
# 1. Update ADR status field
# 2. Update this registry (move row from OPEN to DECIDED)
# 3. Announce in release notes
```

---

## References

- Architecture Decision Records: `docs/adr/`
- Roadmap: `.codex/prompts/workspace-restructure-roadmap.md`
- Criticality Matrix: `docs/CRITICALITY_MATRIX.md`
- GPU Policy: Custom instructions
- QUIC Policy: Custom instructions
