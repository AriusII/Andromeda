# Andromeda 2026 Implementation Roadmap

**Date:** 2026-05-08
**Scope:** Versioned implementation roadmap for the local Rust workspace after consolidation status refresh.

## Operating Position

Andromeda has moved beyond a paper design. The repository contains a structured Rust workspace (94 declared crates), a recoverable `Inventory.ReserveStock` vertical slice, typed Procedure contracts, SRPL compiler components, transaction and WAL contracts, page and heap storage modules, QUIC and Protobuf protocol surfaces, IAM primitives, observability events, benchmark scaffolding, and HA/DR plus PITR contracts. Several declared crates remain scaffolds, runtime-free vocabulary surfaces, or compatibility facades.

The system is still not production SGBDRT. The next work must consolidate contracts into one durable execution path:

```text
Procedure -> Catalog -> SRPL IR -> Admission -> Transaction -> WAL -> Heap/Page -> Recovery -> ResultStream
```

The main risk is not lack of features. The main risk is integration drift: contracts, versions, audit, WAL, storage, and recovery must remain tied together as the runtime becomes real. In the current consolidation posture, `cargo check --workspace --all-targets --all-features` is treated as build continuity only. It is not release approval and not C5 crash/recovery approval. Remaining required gates are explicit: clippy, nextest, doctest, audit, deny, sustained fuzz, Miri, Loom, combined C5 crash/recovery, and full release gate chain evidence.

## Non-Negotiable Guardrails

- No ad hoc SQL application surface.
- No gRPC protocol surface.
- No generic crate ownership buckets named `common`, `utils`, `misc`, `helpers`, or `god_engine`.
- No runtime JSON default for typed protocol payloads.
- `andromeda-rpc-protocol` remains runtime-free and does not depend on QUIC runtime crates.
- `andromeda-quic` owns the concrete QUIC runtime boundary and must not own Procedure semantics, storage truth, or authorization policy.
- `andromeda-security-contract` is runtime-free security vocabulary, not IAM runtime or durable policy storage.
- Every application execution goes through a cataloged Procedure.
- Every Procedure has a typed, hashed, versioned contract.
- No transaction starts before `SecurityAdmission v0` validates protocol, contract, surface, principal/policy evidence, resource budget, and audit evidence.
- No visible mutation exists without durable WAL.
- RAM and HotStore are not system truth.
- Reconstructible truth is the last valid cold snapshot plus durable WAL.
- GPU never participates in commit, rollback, WAL, recovery, MVCC visibility, or security-critical paths.
- Predictive evidence is advisory and must not decide alone.
- Active plans are tied to `CatalogVersion + StatsVersion + ContractHash`.
- Critical decisions require durable, explainable trace evidence.

## Completed 20-Worker Phase

The completed 20-worker phase produced a broad set of implementation changes. These changes are valuable, but they are not yet a single accepted production milestone until the workspace gates pass and the integration wave closes the remaining edges.

| Area | Completed worker outcome | Remaining integration risk |
|---|---|---|
| SRPL DefinitionBatch | SRPL sources can dry-run into Procedure manifests with all-or-nothing rejection. | Broader SRPL runtime dispatch and catalog binding still need end-to-end use. |
| SRPL diagnostics | Forbidden construct scanning is UTF-8 safe and preserves byte spans. | Expand diagnostics as the V0 grammar grows. |
| Catalog contracts | `ProcedureContractBinding` validates contract, catalog, statistics, and policy versions. | Ensure all runtime paths use the validated binding, not partial identity. |
| Execution admission | Pre-transaction gates reject missing, legacy, or divergent binding evidence. | Finish durable table integration and Procedure Store emission. |
| Protobuf manifest | Procedure manifest projection includes binding evidence including `StatsVersion`. | Compatibility policy for old manifests remains open. |
| QUIC routing | Route binding validates surface, contract, catalog, manifest, and frame context before dispatch. | Wire to the default QUIC application server when that server becomes real. |
| Heap/page format | Heap page V1 offset and slot layout are locked and tested. | Legacy raw heap fixtures may require migration or rejection policy. |
| BufferPool WAL fence | Dirty page flush checks observed durable WAL coverage. | Keep this invariant through DiskPageStore and checkpoint integration. |
| ProductStock heap API | `ProductStock` rows have deterministic heap encoding and tests. | Execution must use this durable path as the source of truth. |
| Heap recovery | Committed heap redo records can reconstruct state deterministically. | Vertical crash tests must prove table state reconstruction. |
| Catalog WAL bridge | Catalog publication replay is complete-sequence and all-or-nothing. | Catalog payload replay must be wired to the durable catalog store. |
| Transaction commit log | Commit and rollback terminal records carry durable LSN evidence. | Storage and execution must return true durable-prefix evidence. |
| Execution ProductStock adapter | Execution has an adapter boundary for durable stock publication. | Replace observed in-memory paths with durable heap/table integration. |
| Procedure Store | Invocation runtime records capture binding, timing, plan, row, WAL, temp, and error evidence. | Execution must emit records for committed, failed, and aborted outcomes. |
| Statistics publication | Candidate `StatsVersion` validation and active switch are explicit and traced. | Optimizer consumption and rollback policy still need runtime wiring. |
| PlanCache gate | PlanCache identity is bounded and versioned with advisory evidence only. | This is not a full optimizer. Do not overstate it as one. |
| Security admission and IAM primitives | `SecurityAdmission v0` is documented as a pre-transaction contract over surface, principal, policy, resource, and audit evidence; certificate identity, principal status, permissions, and surface scopes exist. | Full durable IAM registries and policy-management runtime remain pending. |
| Durable audit | `AuditLedger v0` journal evidence is append-only at the record layer, and checksum-chain validation detects missing or corrupted records. | Legacy journal compatibility or migration policy remains open; retention compaction must preserve retained payload checksum evidence and is not the transaction commit path. |
| Benchmark evidence | Scenario evidence is bounded, expirable, and non-authoritative. | Catalog/optimizer publication remains pending. |
| HA/DR and PITR | Majority, promotion, WAL range, and exact target LSN gates are tested. | Full backup/restore drills and cluster simulation remain future work. |

## Current Consolidation Work

The consolidation wave was intentionally narrower than the 20-worker fan-out. Its purpose was to make the work coherent, preserve build continuity, close high-value test gaps, and keep the documentation factual. Read-only documentation workers are now closed.

| Lot | Scope | Deliverable | Exit criteria |
|---|---|---|---|
| A | Workspace compile stabilization | Keep API fallout from worker changes resolved. | Build continuity is observed; full workspace gate chain remains required. |
| B | Durable vertical path | Connect `Inventory.ProductStock` execution to heap/page/WAL/recovery. | Crash/recovery tests reconstruct stock state. |
| C | Catalog and planning evidence | Wire Procedure Store, statistics publication, and PlanCache identity into runtime paths. | Invocation and plan evidence include contract, catalog, statistics, and policy versions. |
| D | Transaction, HA/DR, and PITR proof | Strengthen commit-log durable LSN evidence, promotion fencing, quorum, and PITR WAL coverage gates. | Commit/rollback terminal records prove durable LSN coverage; promotion and restore decisions are deterministic and explainable. |
| E | QUIC, IAM, audit, and recovery integration edges | Connect route binding, principal checks, audit evidence, and catalog/heap replay decisions into runtime paths. | Wrong surface, disabled principal, or missing permission creates no transaction, emits audit evidence, and preserves recovery explainability. |
| WR-5.DOC | Lot 5 docs and ADR acceptance alignment | Align DEC-040/041 index, Lot 5 acceptance, `SecurityAdmission v0`, `AuditLedger v0`, RPC/QUIC/security-contract vocabulary, and audit CLI wording. | Documentation only; no Rust code; no implemented Admin RPC query claim. |
| G/L | Doctrine and release scan | Run doctrine scanners, focused policy gates, compile gate, and release gate checks. | Doctrine and policy scans remain useful, but no release-readiness claim is valid until the full gate chain is rerun with retained evidence. |
| J | Test gap matrix | Identify missing deterministic tests and add only low-conflict tests in owning crates. | Added tests prioritize WAL-before-visible-commit, catalog/version binding, audit chain, and recovery replay. |
| H | Documentation consolidation | Update current state, roadmap, and worker matrix only. | Documents distinguish implemented, partially integrated, validated, and residual-risk work without touching production code. |
| WR-GOV-TOPO-DOC | Lots 1-2-3 transversal topology and documentation governance | Strengthen topology guards for R0 foundation, Lot 2 protocol/catalog/security-contract boundaries, and Lot 3 SRPL dev-dependency drift without touching business crates. | `common`/`utils`/`misc`/`helpers`/`god_engine` crates are rejected; `andromeda-security-contract` is documented as runtime-free R1 vocabulary; `andromeda-quic` to `andromeda-rpc-protocol` and `andromeda-srpl` facade exit criteria are explicit; targeted CLI topology gates are the acceptance checks. |
| N | Documentation stale-blocker audit | Review current state, roadmap, and worker matrix for outdated blocker claims. | Documents mark the previous CLI benchmark error mapping issue as resolved in the latest compile report. |

## Lot 5 Acceptance Alignment

WR-5.DOC aligns documentation with DEC-040 and DEC-041. Lot 5 acceptance is limited to the following claims unless code evidence proves more:

| Area | Accepted claim | Claim to avoid |
|---|---|---|
| `SecurityAdmission v0` | Pre-transaction contract boundary over protocol, contract, surface, principal/policy evidence, resource budget, and audit evidence. | Full durable IAM implementation or policy-management runtime. |
| `AuditLedger v0` | Append-only, checksum-chained durable audit record evidence with retention compaction caveats. | Database truth, transaction commit path, or immutable bytes across compaction. |
| RPC protocol | `andromeda-rpc-protocol` is runtime-free frame, stream, codec, and sequencing contract ownership. | QUIC listener runtime or transport adapter ownership. |
| QUIC runtime | `andromeda-quic` owns feature-gated concrete transport behavior. | Procedure semantics, storage truth, or authorization policy authority. |
| Security contract | `andromeda-security-contract` owns runtime-free vocabulary and explicit semantic mappings. | IAM runtime, durable `PrincipalRegistry`, revocation store, or policy store. |
| Audit tooling | Current operator wording uses `audit inspect`, `audit verify`, and `audit compact`. | Implemented Admin RPC audit query endpoint or legacy `audit query` examples. |

Normative WR-5.DOC acceptance references:

- `docs/adr/ADR-0012-quic-rpc-no-grpc.md`
- `documentations/specs/FrameHeader_RPC_v0.md`
- `documentations/specs/SecurityAdmission_v0.md`
- `documentations/specs/AuditLedger_v0.md`


## Priority Roadmap

### P0 - Stabilize The Integrated Vertical Path

P0 is the minimum path to a credible recoverable database prototype.

1. Keep workspace gates green as implementation batches are reviewed and packaged.
2. Make `Inventory.ProductStock` durable through heap/page storage as the default source of truth.
3. Prove recovery from durable WAL reconstructs `Inventory.ProductStock` end to end.
4. Make catalog DefinitionBatch publication WAL-covered and replayable through the durable catalog store.
5. Use full `ProcedureContractBinding` everywhere an Invocation, plan, audit record, manifest, or benchmark evidence item is created.
6. Emit Procedure Store runtime records from all execution terminal outcomes.
7. Persist `SecurityAdmission v0` rejection evidence for QUIC, surface, principal, and policy failures into durable audit where the owning implementation proves the path.
8. Keep doctrine scans active for no gRPC, no ad hoc SQL application surface, no runtime JSON default, and no GPU critical-path use.

### P1 - Make Procedure Execution Generic

P1 replaces special-case vertical behavior with generic catalog-backed execution.

1. Make `ProcedureRegistry` catalog-backed.
2. Dispatch by Procedure identity, contract hash, and typed payload.
3. Require admission order: protocol validation, contract validation, `SecurityAdmission v0` validation, resource budget, then transaction creation.
4. Lower SRPL IR to minimal physical operators.
5. Emit ResultStream metadata before payload batches.
6. Preserve exact row-count policy in ResultStream metadata.
7. Route business, contract, transaction, resource, and system failures through typed errors.
8. Record Invocation duration, rows read/written, WAL bytes, temp bytes, plan identity, and error kind.

### P2 - Complete Durable Storage V0

P2 turns the storage foundation into a complete minimal durable storage path.

1. Finish DiskPageStore read/write/fsync behavior.
2. Integrate BufferPool fetch, pin, unpin, dirty tracking, and flush.
3. Enforce WAL-before-page-flush for every dirty page.
4. Add checkpoint begin/end records and recovery-window updates.
5. Publish cold snapshots through validated manifest switches.
6. Reject torn writes and corrupt page trailers.
7. Rebuild heap and access-path skeletons during recovery.
8. Add deterministic crash drills for WAL truncation, page corruption, manifest corruption, incomplete snapshot staging, and incomplete catalog batches.

### P3 - Strengthen Transactions And MVCC

P3 makes isolation and rollback observable on durable state, not only in the state machine.

1. Connect `TransactionManager` to heap/page runtime.
2. Acquire and release snapshots through `ActiveSnapshotRegistry`.
3. Apply MVCC row visibility rules to durable row versions.
4. Track write sets and rollback evidence.
5. Add strict write locking for critical writes.
6. Define V0 isolation policies for snapshot and serializable Procedures.
7. Implement MVCC garbage collection with pins for active snapshots, backups, replica lag, and forensic retention.
8. Add deadlock, timeout, and retry semantics with trace evidence.

### P4 - Bring Up QUIC Application Runtime

P4 moves from local protocol smoke checks to a minimal networked application runtime.

1. Keep `runtime-quinn` feature-gated until tests are stable.
2. Keep `andromeda-rpc-protocol` runtime-free while QUIC runtime work happens in `andromeda-quic`.
3. Implement application frames for hello, authentication, contract request, execution request, metadata, payload batch, completion, and typed error.
4. Use mTLS development certificates only in local tests.
5. Keep Application, Admin, and Cluster surfaces separated.
6. Reject Admin or Cluster frames on the Application surface.
7. Enforce backpressure with bounded buffers and batch sizing.
8. Reject mutation admission through unsafe early-data paths.
9. Add malformed frame, length, header checksum, and payload-kind tests.

### P5 - Make Security Runtime And Audit Durable

P5 turns security primitives into durable system behavior. It does not turn DEC-041 security contract vocabulary into an IAM runtime claim.

1. Store `UserPrincipal`, `CertificateIdentity`, roles, permissions, and policies in the system catalog.
2. Validate the chain: certificate identity, principal, roles, permissions, policies, audit.
3. Add minimum permissions for Procedure execution, contract read, DefinitionBatch import, backup, restore, cluster promotion, audit read, and plan inspection.
4. Add policy scope for surface, namespace, tenant, resource budget, time window, and break-glass.
5. Ensure disabled principals fail before transaction creation.
6. Keep `AuditLedger v0` append-only at the record layer, checksum chained, durable, retained by policy, and eventually readable through an explicitly implemented Admin surface. Retention compaction may rewrite retained journal records with rethreaded chain evidence.
7. Ensure audit cannot be globally disabled.
8. Do not document an Admin RPC audit query as implemented until code proves that endpoint.

### P6 - Publish Statistics And Minimal Optimization

P6 adds cost-based planning without learned or predictive components deciding alone.

1. Collect CPU statistics for row counts, histograms, NDV, density, and skew.
2. Publish candidate `StatsVersion` only through validation and trace.
3. Consume statistics in a bounded minimal optimizer.
4. Enumerate a small, bounded set of physical plan classes.
5. Include CPU, logical IO, physical IO, WAL, temp, network, and risk cost.
6. Cache plans only by strict `PlanCacheKey`.
7. Explain candidates considered, candidates rejected, statistics used, evidence ignored, and final plan.
8. Add hysteresis to prevent plan flapping.

### P7 - Add Benchmark Evidence Safely

P7 converts diagnostics into advisory evidence.

1. Store benchmark history durably.
2. Generate `ScenarioEvidence` only from bounded workloads.
3. Tie evidence to Procedure, contract, statistics, and plan class.
4. Expire evidence by time, version, and confidence.
5. Detect regressions against explicit budgets.
6. Keep evidence non-authoritative.
7. Record when optimizer ignores evidence and why.
8. Add poisoning and drift defenses before evidence affects planning.

### P8 - Build Maps And Analytics Before GPU

P8 adds analytical functionality while preserving the commit path.

1. Add `MapDescriptor` for definition IR, grain, refresh policy, storage layout, staleness, and statistics.
2. Support immediate Maps only when bounded and transactionally safe.
3. Support incremental Maps through delta logs, validation, and publish switches.
4. Support deferred and snapshot-only Maps for larger work.
5. Add columnar segment layout for analytical Maps.
6. Enforce summarizability by declared grain and compatible aggregates.
7. Prevent Map maintenance from starving WAL, checkpoint, or foreground transaction work.
8. Keep GPU absent from this phase unless CPU paths are stable.

### P9 - Keep GPU Optional And Off Critical Paths

GPU work remains future-facing and optional.

1. Introduce a separate optional GPU crate only after CPU statistics and Maps are stable.
2. Gate every GPU use through pipeline policy.
3. Allow only statistics refresh, Map refresh, batch analytics, or bounded vector workloads.
4. Require CPU fallback.
5. Require validation evidence for GPU-produced advisory data.
6. Record device, kernel, duration, transfer bytes, validation status, and fallback reason.
7. Add global and scoped kill switches.
8. Prove no commit, WAL, rollback, recovery, MVCC visibility, or security module imports GPU runtime code.

### P10 - HA/DR, Backup, PITR, And Forensics

P10 moves availability and recovery from contracts to drills.

1. Archive WAL segments with hashes and retention policy.
2. Execute backup plans from cold snapshot, WAL archives, catalog, audit, and manifests.
3. Restore to exact LSN or timestamp.
4. Simulate primary, replica, witness, lag, fencing, and promotion.
5. Reject promotion without quorum and durable LSN eligibility.
6. Keep ForensicStart read-only for application traffic.
7. Produce recovery and coherence reports.
8. Run restore drills as tests, not manual procedures.

## Near-Term Task Queue

1. Keep the CLI benchmark error mapping exhaustive as benchmark temp-budget variants evolve.
2. Re-run the full workspace gates after every accepted integration batch.
3. Keep `workspace_dependency_topology` and `orphan_source_invariants` active for R0, Lot 2, Lot 3, protocol, security-contract, and anti-pattern crate-name drift.
4. Connect the execution `ProductStock` adapter to the durable heap API as the default path.
5. Add crash tests for committed ProductStock heap state replay through the vertical path.
6. Wire Procedure Store runtime record emission in execution.
7. Ensure contract mismatch, disabled principal, wrong surface, and missing permission create no transaction.
8. Persist admission rejection decisions to durable audit.
9. Connect catalog WAL bridge replay to durable catalog store publication.
10. Add DefinitionBatch crash tests for begin/apply/commit boundaries.
11. Add compatibility policy for old Protobuf manifests without `StatsVersion`.
12. Add compatibility policy for old durable audit journals without checksum-chain fields.
13. Publish basic table statistics for the durable ProductStock path.
14. Feed published `StatsVersion` into PlanCache identity and DecisionTrace.
15. Keep ScenarioEvidence advisory and version-bound.
16. Review and package the broad dirty worktree into coherent PR-sized batches.

## Open Decisions

| Decision | Options | Current recommendation |
|---|---|---|
| Optimizer placement | Keep inside catalog or create `andromeda-optimizer` | Keep identity types in catalog; split a crate when physical planning grows. |
| GPU placement | Core module or optional crate | Use an optional crate to prevent accidental critical-path imports. |
| Legacy Protobuf manifest handling | Reject, migrate, or dual-read | Reject by default until a migration policy is written. |
| Legacy audit journal handling | Reject, migrate, or dual-read | Reject by default for corruption safety; add explicit migrator if needed. |
| Admin audit read surface shape | CLI-only inspection, Admin RPC endpoint, or both | Use current CLI `audit inspect`/`audit verify`/`audit compact` wording until an Admin RPC endpoint is implemented and tested. |
| Page size policy | 16 KiB, 32 KiB, or mixed | Keep current V1 format fixed; add policy later for cold/analytics paths. |
| Serializable V0 | Strict 2PL, SSI scaffold, or deterministic scheduling | Use strict locking for critical writes first; add SSI later. |
| Catalog version granularity | Database, instance, or hybrid | Use database-scoped catalog versions with global system-catalog versions for registries. |

## Remaining Validation Gates

The following gates are still required and are not treated as complete by this
roadmap refresh:

- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo nextest run --workspace --all-features`
- `cargo test --doc --workspace`
- `cargo audit`
- `cargo deny check`
- Sustained fuzz campaigns with retained artifacts
- Targeted Miri runs where applicable
- Targeted Loom runs where applicable
- Combined C5 crash/recovery matrix evidence
- Release gate chain evidence package

## Final Direction

The correct order remains:

```text
1. Truth and recovery.
2. Catalog and contracts.
3. SRPL and transaction-scoped execution.
4. Secure RPC.
5. Statistics and optimizer.
6. Maps and analytics.
7. Benchmark evidence.
8. Optional GPU acceleration.
9. HA/DR and production operations.
```

Do not shortcut toward GPU, learned optimization, or broad analytics before the durable Procedure path is real. The next meaningful milestone is a minimal, testable, recoverable path where a cataloged Procedure mutates durable heap/page state, publishes only after durable WAL, emits typed ResultStream data, and can be reconstructed after crash.
