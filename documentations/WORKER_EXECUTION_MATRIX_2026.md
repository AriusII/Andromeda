# Andromeda Worker Execution Matrix

**Date:** 2026-05-07  
**Scope:** Worker ownership, integration order, and validation status for the completed 20-worker phase plus the current consolidation work.

## Operating Rules

- Workers keep changes scoped to their assigned ownership area.
- Workers do not revert user changes or changes made by another worker.
- Documentation workers do not touch code.
- Every code change identifies focused tests and expected workspace gates.
- Doctrine remains active: no gRPC, no ad hoc SQL application surface, no runtime JSON default, no unsafe drift, and no GPU on commit, WAL, rollback, recovery, MVCC visibility, or security-critical paths.
- Release blockers remain active: visible commit without durable WAL, accepted contract mismatch, recovery failure, audit omission, panic in a critical path, silent corruption, non-reproducible crash tests, or unobservable critical decisions.

## Completed 20-Worker Phase

| Worker | Focus | Primary ownership | Completed outcome | Follow-up integration |
|---|---|---|---|---|
| 01 | SRPL DefinitionBatch source dry-run | `crates/andromeda-srpl` | SRPL sources dry-run to Procedure manifests with all-or-nothing rejection. | Use the dry-run bridge in catalog DefinitionBatch publication. |
| 02 | SRPL diagnostics | `crates/andromeda-srpl/src/source_location.rs` | Forbidden scanner is UTF-8 safe and preserves byte spans. | Extend as SRPL V0 grammar grows. |
| 03 | Catalog `ProcedureContractBinding` validation | `crates/andromeda-catalog/src/contracts` | Binding construction and materialization reject incomplete or divergent evidence. | Ensure every runtime caller uses validated binding. |
| 04 | Execution pre-transaction binding gate | `crates/andromeda-exec` | Execution rejects missing, legacy, or divergent binding before transaction scope creation. | Connect IAM and audit rejection evidence in the same pre-transaction path. |
| 05 | Protobuf manifest binding | `crates/andromeda-proto` | Procedure manifest projection carries binding evidence including `StatsVersion`. | Decide compatibility for old manifests without `StatsVersion`. |
| 06 | QUIC application route binding | `crates/andromeda-quic` | Route binding rejects wrong surface, scope, hash, catalog, and frame context before dispatch. | Wire into the default QUIC application runtime when enabled. |
| 07 | Heap/page format lock | `crates/andromeda-storage/src/heap` | Heap V1 payload offset, slot directory, and page/trailer checks are locked. | Define legacy raw heap fixture migration or rejection policy. |
| 08 | DiskPageStore and BufferPool WAL fence | `crates/andromeda-storage/src/disk_manager`, `buffer_pool` | Dirty flush requires observed durable WAL coverage and leaves failed pages retryable. | Preserve the fence through checkpoint and recovery integration. |
| 09 | Durable `Inventory.ProductStock` heap API | `crates/andromeda-storage` | ProductStock rows have deterministic heap encoding and typed insert/read/scan tests. | Connect execution vertical path to this durable store. |
| 10 | Heap redo and recovery reconstruction | `crates/andromeda-storage/src/recovery` | Committed HREDOV1 heap records replay deterministically and malformed records fail closed. | Add vertical crash tests that reconstruct ProductStock state. |
| 11 | Catalog WAL bridge replay | `crates/andromeda-storage/src/catalog_wal_bridge.rs` | Complete begin/apply/commit publication sequences replay all-or-nothing. | Wire payload application into durable catalog store recovery. |
| 12 | Transaction durable LSN evidence | `crates/andromeda-tx` | Commit and rollback terminal records carry explicit durable LSN evidence. | Ensure storage and execution return true durable-prefix values. |
| 13 | Vertical V0 durable table preparation | `crates/andromeda-exec` | Execution has a ProductStock store boundary and commit evidence model. | Replace observed-stock behavior with durable heap/table source of truth. |
| 14 | Procedure Store runtime records | `crates/andromeda-catalog/src/procedure_store` | Invocation runtime records capture binding, timing, rows, WAL/temp bytes, plan, and error evidence. | Emit records from execution terminal outcomes. |
| 15 | `StatsVersion` publication switch | `crates/andromeda-catalog/src/statistics` | Candidate statistics validate before active publication and emit decision trace. | Feed active statistics into optimizer and PlanCache runtime paths. |
| 16 | Minimal PlanCache gate | `crates/andromeda-catalog/src/plan_cache.rs` | PlanCache key is bounded by Procedure, contract, catalog, statistics, policy, plan class, and shape. | Keep it scoped as identity gate until optimizer runtime exists. |
| 17 | IAM principal and certificate primitives | `crates/andromeda-core/src/principal` | Certificate identity, principal status, registry, permission, and surface scope checks exist. | Wire durable registries into catalog, QUIC, execution, and audit. |
| 18 | Durable audit checksum chain | `crates/andromeda-observe/src/events/durable_audit` | Audit journal lines include previous/current checksum chaining and replay validation. | Decide legacy journal migration or rejection policy. |
| 19 | Benchmark evidence boundary | `crates/andromeda-bench` | ScenarioEvidence is bounded, expirable, version-bound, and non-authoritative. | Keep CLI benchmark error mapping exhaustive and publish only advisory evidence. |
| 20 | HA/DR backup and PITR gate | `crates/andromeda-storage/src/hadr`, restore orchestration | Majority, promotion eligibility, WAL range, and target LSN gates are tested. | Add full backup/restore drills and cluster simulation. |

## Current Consolidation Work

| Lot | Role focus | Write scope | Required skills | Output evidence |
|---|---|---|---|---|
| A | Build and API consolidation | Narrow code fixes needed to keep workspace compile green | Rust core review, test matrix generation | Workspace format, check, build, test, and clippy gates pass after consolidation. |
| B | Durable vertical integration | Execution/storage tests and ProductStock wiring | Procedure contract design, WAL record design, recovery replay proof | `vertical-v0` uses durable ProductStock state and crash replay proves reconstruction. |
| C | Catalog and planning evidence integration | Catalog, Procedure Store, statistics, PlanCache wiring | Catalog object modeling, statistics histogram design, optimizer PlanClass design | Runtime records include contract, catalog, statistics, policy, plan, and row evidence. |
| D | Transaction and HA/DR consolidation | `crates/andromeda-tx/**`, `crates/andromeda-storage/src/hadr/**`, `crates/andromeda-storage/src/restore_orchestration.rs`, and related HADR/restore tests only | Transaction WAL recovery review, transaction state machine, HA/DR quorum review, backup PITR runbook, HA/DR backup forensic runbook | Commit and rollback terminal records require durable LSN evidence; promotion and restore gates validate quorum, fencing, and WAL coverage. |
| E | QUIC, IAM, audit, and recovery integration edges | QUIC route binding, principal checks, audit emission, catalog/heap replay evidence | QUIC frame design, security mTLS/IAM review, audit trace specification, recovery replay proof | Wrong surface or denied identity fails before transaction creation, records audit evidence, and preserves recovery explainability. |
| G/L | Doctrine and release scan | Doctrine scanners, focused policy gates, compile gate, and release gate reporting | Project invariant check, no gRPC enforcement, no SQL surface scan, no JSON runtime policy | Doctrine scans, Protobuf doctrine scanner, no-gRPC scanner, Codex tooling validation, and focused policy gate pass without false-positive release blockers. |
| J | Test gap matrix | Tests in owning crates; production code only if a compile issue blocks a test | Test matrix generation, crash recovery test design, testing crash recovery matrix, formal invariant ledger | High-value missing tests are identified. Low-conflict deterministic tests are added for WAL-before-visible-commit, catalog/version binding, audit chain, and recovery replay where clear. |
| H | Roadmap and current-state consolidation | `docs/CURRENT_STATE.md`, `docs/ROADMAP_IMPLEMENTATION_2026.md`, `docs/WORKER_EXECUTION_MATRIX_2026.md`; optional factual cross-reference correction in `docs/ANDROMEDA_ROADMAP_IMPLEMENTATION_CROSSCHECK_2026.md` | Microsoft doc style edit, terminology normalization, project invariant check, agent handoff contracting, test matrix generation | Documentation reflects completed, partially integrated, validated, and residual-risk work using Andromeda terminology. |
| N | Documentation stale-blocker audit | `docs/CURRENT_STATE.md`, `docs/ROADMAP_IMPLEMENTATION_2026.md`, `docs/WORKER_EXECUTION_MATRIX_2026.md` only | Microsoft doc style edit, terminology normalization, agent output validation | Documentation marks the previous CLI benchmark error mapping issue as resolved in the latest compile report and keeps gate statements tied to the last recorded validation evidence. |

## Integration Order

1. Stabilize compile and API compatibility before broad behavioral validation.
2. Accept contract binding changes before execution, protocol, optimizer, and benchmark evidence changes.
3. Accept storage format locks before heap API, BufferPool flush, and recovery replay changes.
4. Integrate durable ProductStock only after storage API and recovery tests are passing.
5. Validate transaction and HA/DR/PITR gates before broad restore or cluster claims.
6. Wire Procedure Store and statistics evidence after execution has stable terminal outcomes.
7. Run focused crate tests after each accepted cluster.
8. Run workspace gates only after compile is restored.

## Workspace Gates

```powershell
cargo fmt --all -- --check
cargo check --workspace --quiet
cargo test --workspace --quiet
cargo clippy --workspace --all-targets
```

## Known Gate State

| Gate | Last recorded state | Owner |
|---|---|---|
| `cargo fmt --all -- --check` | Passed on 2026-05-07 after consolidation cleanup. | Re-run after each packaging batch. |
| `cargo check --workspace --quiet` | Passed on 2026-05-07 after consolidation cleanup. | Re-run after each packaging batch. |
| `cargo build --workspace` | Passed on 2026-05-07 after consolidation cleanup. | Re-run before release packaging. |
| `cargo test --workspace --quiet` | Passed on 2026-05-07 after consolidation cleanup. | Re-run after each packaging batch. |
| `cargo clippy --workspace --all-targets` | Passed on 2026-05-07 with no warnings reported. | Re-run before release packaging. |

## Documentation Rules For Worker H

- Use American English in repository documents.
- Prefer Procedure, Invocation, Procedure Store, Map, Modelization, DefinitionBatch, ResultStream, and StructuredObject.
- Do not claim code lots are complete unless validation evidence exists.
- Separate completed 20-worker outcomes from current consolidation work.
- Record residual risks and next actions instead of presenting scaffolds as production runtime.

## Documentation Rules For Worker N

- Edit only the current state, implementation roadmap, and worker execution matrix.
- Treat the previous CLI benchmark error mapping blocker as resolved and covered by CLI benchmark tests unless a new Cargo run proves otherwise.
- Keep Cargo gate statements tied to the command and date that produced them.
- Do not run broad Cargo gates for documentation-only stale-blocker cleanup.

## Current Worktree Risk Register

| Risk | Severity | Likelihood | Mitigation | Owner | Review trigger | Status |
|---|---|---|---|---|---|---|
| Dirty worktree hides integration regressions across many crates. | High | Medium | Keep compile green after each accepted code lot and run focused owning-crate tests before workspace tests. | Lot A plus owning domain lots | Any new compile error, broad conflict, or unreviewed API drift. | Active |
| New runtime evidence types are partially integrated but not emitted everywhere. | High | Medium | Track full `ProcedureContractBinding`, `StatsVersion`, `PolicyVersion`, PlanCache identity, audit evidence, and Procedure Store records across admission and terminal outcomes. | Lots C and E | Any Invocation path creates transaction scope or terminal outcome without full evidence. | Active |
| WAL, heap, catalog, and recovery proofs can pass separately but fail end to end. | High | Medium | Worker J and recovery-owning lots add deterministic crash/replay tests that join WAL, heap, catalog publication, and recovery reports. | Worker J plus Lots D and E | Recovery test gap or replay report cannot explain accepted/rejected records. | Active |
| Legacy manifest or audit journal records may be rejected after new version/checksum fields. | Medium | Medium | Define explicit reject, migrate, or dual-read policies before compatibility claims. | Lots C and E | Any attempt to read old manifest or audit data in tests or tooling. | Open |
