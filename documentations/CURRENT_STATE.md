# Andromeda Current State

**Date:** 2026-05-07  
**Scope:** Working implementation status after the completed 20-worker phase and the 2026-05-07 consolidation wave.

## Summary

Andromeda is a recoverable vertical prototype with a broad modular foundation. It is not yet a production SGBDRT runtime, a default QUIC application server, or a complete durable storage engine.

The completed 20-worker phase moved several contracts from scaffold to executable checks: SRPL DefinitionBatch dry-run, full `ProcedureContractBinding` validation, Protobuf manifest binding, QUIC route binding, heap/page format locking, WAL-gated page flush, heap redo, catalog WAL replay, transaction durable LSN evidence, Procedure Store runtime records, `StatsVersion` publication switching, bounded PlanCache identity, IAM primitives, durable audit checksum chaining, benchmark `ScenarioEvidence` boundaries, and HA/DR/PITR validation gates.

The 2026-05-07 consolidation wave focused on integration, build stabilization, doctrine validation, test coverage, and documentation. Workers closed protocol, execution, storage, transaction, HA/DR, IAM, audit, catalog evidence, benchmark evidence, policy gate, documentation, and agent/skill governance lots. Documentation workers did not stage, commit, revert unrelated changes, or claim that partial runtime scaffolds are production features.

## Gate Status

Last recorded gate state after consolidation:

| Gate | Status | Notes |
|---|---|---|
| `cargo fmt --all -- --check` | Passed | Re-run on 2026-05-07 after implementation-lot formatting cleanup. |
| `cargo check --workspace --quiet` | Passed | Re-run on 2026-05-07 after the CLI benchmark mapping, restore PITR test, WAL bridge test, and clippy cleanup fixes. |
| `cargo build --workspace` | Passed | Re-run on 2026-05-07 after consolidation. |
| `cargo test --workspace --quiet` | Passed | Re-run on 2026-05-07 after fixing restore PITR range coverage and transaction WAL terminal drift tests. |
| `cargo clippy --workspace --all-targets` | Passed | Re-run on 2026-05-07 with no remaining warnings reported. |

No dedicated Markdown style tool was found in the repository during Worker H validation, and no `markdownlint`, `markdownlint-cli2`, `mdformat`, or `vale` command was available in the shell. Worker N validated this documentation scope with targeted `rg` and PowerShell scans only.

## Worktree State

The worktree is intentionally dirty from prior worker implementation and agent/skill migration work. The dirty state includes broad code changes across engine crates, newly added consolidation agent and skill assets, deleted legacy `.claude` and `.junie` artifacts, and untracked documentation files. Worker H did not stage, commit, revert, or normalize unrelated changes.

## Runtime Today

- The Rust workspace includes crates for core types, catalog, SRPL, transaction, storage, protocol, QUIC, execution, observability, benchmark, and CLI surfaces.
- `vertical-v0` remains the local reference path for `Inventory.ReserveStock`.
- Procedure contract identity is now stronger across catalog, execution, protocol, QUIC, plan cache, benchmark evidence, and audit-facing records.
- SRPL DefinitionBatch dry-run can compile SRPL sources into Procedure manifests and reject invalid sources all-or-nothing.
- Heap/page layout has a locked V1 payload offset and stronger validation for header, trailer, slot directory, and tuple ranges.
- BufferPool and page flush paths now have explicit WAL durability fences.
- Storage has typed `ProductStock` heap row support and heap redo/recovery tests, but the vertical path still needs final durable-table integration.
- Catalog WAL bridge replay can reconstruct complete catalog publication sequences and reject incomplete or malformed tails.
- The transaction commit log carries durable LSN evidence for commit and rollback terminal records.
- Procedure Store runtime records can capture Invocation-level execution evidence, but execution emission is not fully wired.
- `StatsVersion` publication has a candidate-to-active switch with validation and decision trace evidence.
- PlanCache identity is bounded by `ProcedureId + ContractHash + CatalogVersion + StatsVersion + PolicyVersion + PlanClass + ShapeFingerprint`.
- QUIC routing can reject wrong surface, contract, catalog, manifest, or frame context before dispatch.
- IAM primitives exist for certificate identity, principal status, registries, permissions, and surface scope, but durable system catalog wiring remains pending.
- Durable audit journals have checksum chaining, replay verification, and deletion detection.
- Benchmark `ScenarioEvidence` is bounded, expirable, non-authoritative, and tied to Procedure, contract, statistics, and plan class.
- HA/DR quorum and PITR restore validation now check majority, promotion eligibility, WAL ranges, and target LSN boundaries.

## Scaffold Or Partial Runtime

- The default application surface is not yet a complete QUIC server/client runtime.
- The generic SRPL Procedure dispatcher is not yet the only path for all Procedures.
- `Inventory.ProductStock` still needs an end-to-end durable table path through execution, transaction, WAL, page/heap storage, and recovery.
- Catalog durability has WAL replay contracts, but the full catalog store path still needs end-to-end publication and recovery hardening.
- Procedure Store runtime records exist, but execution must emit them on all terminal Invocation outcomes.
- Statistics publication and PlanCache identity exist, but a full cost-based optimizer is still future work.
- IAM primitives are not yet a fully durable system database registry.
- Audit checksum chaining is durable, but legacy journal compatibility or migration policy is still open.
- GPU remains policy-only and must stay outside commit, rollback, WAL, recovery, MVCC visibility, and security-critical paths.

## Current Consolidation Work

| Lot | Focus | Status | Acceptance evidence |
|---|---|---|---|
| A | Workspace compile stabilization | Complete for this wave | Full workspace format, check, build, test, and clippy gates passed on 2026-05-07. |
| B | Durable `Inventory.ProductStock` integration | Consolidated with residual runtime work | Execution rejects permissioned local paths without authorization context before transaction creation; ProductStock prepared state remains invisible without durable commit evidence. |
| C | Catalog, Procedure Store, statistics, and PlanCache wiring | Consolidated with residual durability work | Procedure Store runtime records, `StatsVersion` switch, PlanCache identity, and benchmark `ScenarioEvidence` have focused tests. Durable catalog publication and runtime emission remain follow-up work. |
| D | Transaction, HA/DR, and PITR consolidation | Consolidated | Commit and rollback terminal records require durable LSN evidence; promotion and restore gates validate quorum, fencing, required WAL start, and target boundaries. |
| E | QUIC, IAM, audit, and recovery integration edges | Consolidated with residual audit wiring | QUIC carries expected `StatsVersion`; route/IAM checks fail closed before dispatch; durable audit checksum-chain tests pass. Emitting all authorization decisions into durable audit remains follow-up work. |
| G/L | Doctrine and release scan | Complete for this wave | `andromeda_policy_gate.py`, Protobuf doctrine scan, and no-gRPC policy scan pass. The legacy policy gate was narrowed to avoid documentation and guardrail false positives. |
| J | Test gap matrix | Complete for this wave | Deterministic tests were added for WAL-before-visible-commit, catalog/version binding, audit-chain replay, and recovery replay prefix behavior. |
| H/N | Roadmap, current-state consolidation, and stale-blocker audit | Complete for this wave | Docs no longer present the CLI benchmark mapping issue or formatting diffs as active blockers. |

## Active Doctrine

- Application behavior remains Procedure-only. Do not add an ad hoc SQL application surface.
- Protobuf is the wire contract format. Do not introduce gRPC.
- Every Invocation must validate contract identity before transaction creation.
- A mutation is not visible until WAL covers it durably.
- RAM and HotStore are not system truth. Reconstructible truth is the last valid cold snapshot plus durable WAL.
- Result metadata must precede result payload batches when contractual.
- Predictive evidence may inform optimization, but it must not decide alone.
- Critical decisions must be observable and explainable after the fact.

## Immediate P0 Work

1. Review and package the large dirty worktree into coherent batches before staging.
2. Finish durable `Inventory.ProductStock` as an end-to-end source of truth through execution, transaction, heap/page storage, and recovery.
3. Finish catalog store publication and replay hardening with WAL-covered DefinitionBatch application.
4. Wire Procedure Store runtime records from all execution terminal outcomes.
5. Persist IAM, QUIC route binding, and audit rejection evidence before transaction creation.
6. Keep `ProcedureContractBinding` evidence complete across execution, plan cache, audit, protocol manifests, and benchmark evidence.
7. Define compatibility policy for old Protobuf manifests and durable audit journals.
8. Preserve the GPU boundary as policy-only until CPU statistics and Map refresh paths are stable.
