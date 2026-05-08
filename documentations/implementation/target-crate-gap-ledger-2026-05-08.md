# Target Crate Gap Ledger - 2026-05-08

## Purpose

Record the gap between the external target crate roadmap and the current local
Andromeda branch.

This ledger is a planning artifact. It identifies missing target crates,
current broad owners or partial equivalents, acceptance gates, and the safe
phase in which each group may be considered. It does not approve new crates or
runtime behavior.

## Scope

This document covers:

- Current 32-crate workspace reality before any additional target-crate
  scaffolds are created.
- Target-named scaffold crates that exist locally but are not accepted as final
  canonical owners.
- Target crate names observed in the external restructure roadmap.
- Missing target crates by exact workspace name.
- Current owner or facade likely to contain the responsibility today.
- Acceptance gates before creation, extraction, rename, or promotion.

## Non-goals

- Do not create, remove, rename, move, stage, unstage, or commit any crate.
- Do not decide final crate names where ADRs still need to approve naming.
- Do not treat exact-name absence as proof that functionality is absent.
- Do not treat exact-name presence as proof that ownership is accepted.
- Do not treat broad current owners as acceptable long-term ownership.
- Do not approve C5 extraction while the worktree is dirty.
- Do not normalize `docs/` and `documentations/` path mapping in this packet.
  That mapping is being clarified by a separate documentation owner.
- Do not promote GPU, analytics, benchmark, RAM, temp, or audit evidence as
  database truth.

## Prerequisites

Before acting on any ledger row:

1. Confirm the current packet has a clean, owner-scoped candidate state.
2. Confirm whether an existing current crate is the intended final name or a
   temporary facade.
3. Add direct owner tests before relying on compatibility reexports.
4. Preserve public import compatibility unless an explicit migration packet and
   compatibility gate approve removal.
5. For C5 crates, define behavior locks and crash/recovery gates before moving
   implementation code.
6. For runtime-free crates, prove they stay free of runtime stores, Quinn,
   GPU, benchmark, SQL, runtime JSON default, and native-layout persistence
   dependencies as applicable.

## Current Workspace

The current root workspace declares 32 crates under `crates/`. Treat this as
the branch shape before any additional target-crate scaffold packet:

```text
andromeda-bench
andromeda-catalog
andromeda-cli
andromeda-codec
andromeda-contract
andromeda-core
andromeda-digest
andromeda-error
andromeda-exec
andromeda-hardware
andromeda-maps
andromeda-observe
andromeda-policy
andromeda-procedure-store
andromeda-proto
andromeda-quic
andromeda-resource
andromeda-rpc-protocol
andromeda-security-contract
andromeda-srpl
andromeda-srpl-ast
andromeda-srpl-cardinality
andromeda-srpl-diagnostics
andromeda-srpl-ir
andromeda-srpl-lexer
andromeda-srpl-parser
andromeda-storage
andromeda-structured-object
andromeda-time
andromeda-tx
andromeda-types
andromeda-wal
```

The current branch includes 30 exact target names from the external roadmap.
Six of those exact-name crates are local scaffolds or narrow extractions that
must not be treated as accepted phase ownership:

| Current target-named crate | Current posture | Acceptance caution |
|---|---|---|
| `andromeda-codec` | Provisional foundation scaffold for explicit byte helpers | Does not approve persistent or network byte ownership without golden vectors and malformed-input tests. |
| `andromeda-policy` | Provisional runtime-free policy vocabulary | Does not approve IAM runtime, policy store, revocation, or durable authorization behavior. |
| `andromeda-resource` | Provisional resource-budget vocabulary | Does not approve admission integration or C4/C5 resource enforcement. |
| `andromeda-srpl-lexer` | Narrow SRPL lexical scanner extraction | Does not prove binder, lowering, interpreter, or catalog-store-free topology acceptance. |
| `andromeda-procedure-store` | Provisional runtime-free invocation and evidence vocabulary | Does not approve durable Procedure Store runtime, catalog publication, or execution integration. |
| `andromeda-maps` | Provisional runtime-free Map descriptor scaffold | Does not approve Map engine behavior, publication, refresh, rebuild, rollback, or analytics ownership. |

Two current crates are branch-specific names that are not exact target names in
the external extraction:

| Current crate | Current role | Planning implication |
|---|---|---|
| `andromeda-rpc-protocol` | Runtime-free RPC frame and ResultStream protocol contracts | Treat as a current partial equivalent for some `andromeda-rpc` and `andromeda-rpc-codec` target concerns until ADRs or topology work say otherwise. |
| `andromeda-security-contract` | Runtime-free security vocabulary | Treat as a current partial equivalent for security contract vocabulary only. It is not `andromeda-security`, `andromeda-iam`, or durable audit runtime. |

## Gap Summary

| Category | Count | Interpretation |
|---|---:|---|
| Current workspace crates | 32 | Observed branch shape before additional scaffold packets, not acceptance evidence. |
| External target names present exactly | 30 | Represented by exact crate names under `crates/`, including provisional scaffolds. |
| Current-only branch names | 2 | `andromeda-rpc-protocol` and `andromeda-security-contract` are current partial splits. |
| External target names missing exactly | 60 | Missing target names require owner review before creation, rename, or deferral. |

## Procedure

Use this ledger as follows:

1. Select the target crate group that matches the current packet owner.
2. Confirm whether the target is a production crate, test-support crate, tool,
   optional runtime, or documentation/runbook artifact.
3. Check the current owner or facade listed in the ledger.
4. Define the smallest safe packet: create shell crate, move one module, add
   compatibility reexport, or defer.
5. Run the listed acceptance gates and record exact command results.
6. Update this ledger only in a documentation-owned packet after source
   evidence changes.

## Missing Target Crate Ledger

| Phase | Missing target crates | Current owner or partial equivalent | Acceptance gates before action |
|---|---|---|---|
| Phase 1 - Foundation | `andromeda-test-support` | `andromeda-core`, `andromeda-types`, `andromeda-error`, `andromeda-digest`, `andromeda-hardware`; provisional `andromeda-codec`, `andromeda-policy`, and `andromeda-resource`; crate-local test fixtures | Topology gates prove R0 crates have no engine/runtime ownership; codec work requires explicit endian, length-bound, malformed-input, and golden-vector policy before use by storage or protocol. |
| Phase 2 - Contract and catalog | `andromeda-contract-compat`, `andromeda-catalog-store`, `andromeda-definition-batch`, `andromeda-catalog-recovery`, `andromeda-catalog-diff`, `andromeda-procedure-contract`, `andromeda-client-sdk-gen` | `andromeda-contract`, `andromeda-catalog`, `andromeda-srpl`, `andromeda-proto`, tooling docs | ContractHash golden tests, compatibility matrix, DefinitionBatch dry-run tests, catalog publication WAL coverage, malformed-tail replay rejection, and facade compatibility. |
| Phase 2 - SRPL | `andromeda-srpl-binder`, `andromeda-srpl-lowering`, `andromeda-srpl-execution-adapter`, `andromeda-srpl-interpreter`, `andromeda-srpl-test-fixtures` | `andromeda-srpl`, `andromeda-srpl-lexer`, `andromeda-srpl-parser`, `andromeda-srpl-ast`, `andromeda-srpl-cardinality`, `andromeda-srpl-diagnostics`, `andromeda-srpl-ir` | Parser/model crates stay catalog-store-free; binder and lowering moves keep typed Procedure contracts; SRPL parser, AST, cardinality, diagnostics, IR, lexer, and DefinitionBatch compatibility tests pass. |
| Phase 2 - Runtime-free protocol | `andromeda-rpc`, `andromeda-proto-wire`, `andromeda-rpc-codec` | `andromeda-rpc-protocol`, `andromeda-proto`, `andromeda-quic` compatibility paths | Runtime-free frame ownership, explicit wire codecs, no gRPC surface, no runtime JSON default, malformed frame tests, metadata-before-payload tests, and compatibility import tests. |
| Phase 3 - Surface, security, and operations | `andromeda-quic-runtime-quinn`, `andromeda-security`, `andromeda-iam`, `andromeda-audit`, `andromeda-admin`, `andromeda-forensic`, `andromeda-observability`, `andromeda-runbooks` | `andromeda-quic`, `andromeda-security-contract`, `andromeda-core` principal facade, `andromeda-exec`, `andromeda-observe`, `andromeda-cli`, operations docs | Quinn stays feature-gated; security vocabulary remains runtime-free; IAM, admission, audit, Admin, Cluster, and Application surfaces stay separated; wrong surface, disabled principal, missing permission, malformed frame, and unsafe early-data tests create no transaction and emit required evidence. |
| Phase 4 - Transaction and WAL | `andromeda-transaction`, `andromeda-transaction-log`, `andromeda-mvcc`, `andromeda-locking`, `andromeda-savepoint`, `andromeda-wal-codec` | `andromeda-tx`, `andromeda-wal`, `andromeda-storage`, `andromeda-observe` evidence paths | Define durable commit evidence, rollback evidence, WAL prefix boundaries, and visibility rules; run transaction, WAL owner, WAL codec, recovery replay, and prepared-not-visible crash tests before extracting C5 code. |
| Phase 4 - Storage and recovery | `andromeda-storage-page`, `andromeda-storage-heap`, `andromeda-storage-index`, `andromeda-buffer-pool`, `andromeda-disk-page-store`, `andromeda-manifest`, `andromeda-segment`, `andromeda-recovery`, `andromeda-backup`, `andromeda-restore`, `andromeda-hadr` | `andromeda-storage`, `andromeda-wal`, `andromeda-tx`, `andromeda-cli` operation paths | No C5 move while packet is dirty; require explicit codecs, golden byte vectors, WAL-before-page-flush tests, torn-write rejection, manifest switch recovery, FileWal owner versus storage integration tests, backup/restore drills, PITR exact-LSN proof, quorum and fencing proof. |
| Phase 5 - Execution | `andromeda-execution`, `andromeda-admission`, `andromeda-procedure-runtime`, `andromeda-result-stream`, `andromeda-retry`, `andromeda-business-fixtures`, `andromeda-execution-trace` | `andromeda-exec`, `andromeda-catalog`, provisional `andromeda-procedure-store`, `andromeda-contract`, `andromeda-proto`, `andromeda-observe`, `andromeda-quic` bridge paths | Contract validation, `SecurityAdmission v0`, resource budget, and transaction creation order; generic Procedure dispatch; terminal Procedure Store records; ResultStream metadata before payload; exact row-count policy; retry idempotency evidence. |
| Phase 6 - Statistics and optimizer | `andromeda-statistics`, `andromeda-optimizer`, `andromeda-plan-cache` | `andromeda-catalog`, `andromeda-srpl`, `andromeda-bench` advisory evidence paths | `StatsVersion` publication validation, bounded plan classes, strict `PlanCacheKey`, DecisionTrace, evidence ignored when stale or unsafe, plan-flapping hysteresis, no learned or benchmark evidence as sole authority. |
| Phase 6 - Maps, analytics, benchmark, and GPU later | `andromeda-analytics`, `andromeda-columnar`, `andromeda-bench-workload`, `andromeda-bench-harness`, `andromeda-scenario-evidence`, `andromeda-regression`, `andromeda-gpu`, `andromeda-vector`, `andromeda-simd` | Provisional `andromeda-maps`, `andromeda-catalog`, `andromeda-storage`, `andromeda-bench`, `andromeda-hardware`, future optional crates | Durable Procedure path and CPU statistics must be stable first; Maps enforce grain and summarizability; ScenarioEvidence remains advisory; GPU is optional, disabled by default, CPU-fallback capable, and import-scanned out of commit, WAL, rollback, recovery, MVCC visibility, catalog publication, and security-critical paths. |

## Exact Missing Target Names

The external roadmap target names missing from the current workspace by exact
crate name are:

```text
andromeda-admin
andromeda-admission
andromeda-analytics
andromeda-audit
andromeda-backup
andromeda-bench-harness
andromeda-bench-workload
andromeda-buffer-pool
andromeda-business-fixtures
andromeda-catalog-diff
andromeda-catalog-recovery
andromeda-catalog-store
andromeda-client-sdk-gen
andromeda-columnar
andromeda-contract-compat
andromeda-definition-batch
andromeda-disk-page-store
andromeda-execution
andromeda-execution-trace
andromeda-forensic
andromeda-gpu
andromeda-hadr
andromeda-iam
andromeda-locking
andromeda-manifest
andromeda-mvcc
andromeda-observability
andromeda-optimizer
andromeda-plan-cache
andromeda-procedure-contract
andromeda-procedure-runtime
andromeda-proto-wire
andromeda-quic-runtime-quinn
andromeda-recovery
andromeda-regression
andromeda-restore
andromeda-result-stream
andromeda-retry
andromeda-rpc
andromeda-rpc-codec
andromeda-runbooks
andromeda-savepoint
andromeda-scenario-evidence
andromeda-security
andromeda-segment
andromeda-simd
andromeda-srpl-binder
andromeda-srpl-execution-adapter
andromeda-srpl-interpreter
andromeda-srpl-lowering
andromeda-srpl-test-fixtures
andromeda-statistics
andromeda-storage-heap
andromeda-storage-index
andromeda-storage-page
andromeda-test-support
andromeda-transaction
andromeda-transaction-log
andromeda-vector
andromeda-wal-codec
```

## Current Broad Owners To Reduce

These current crates remain broad and should be reduced only through
owner-scoped packets:

| Current crate | Broad responsibility today | Reduction constraint |
|---|---|---|
| `andromeda-core` | Compatibility facade plus principal and foundation remnants | Keep thin; migrate callers to precise foundation or security identity crates only after topology gates. |
| `andromeda-catalog` | Catalog, DefinitionBatch, Procedure Store runtime modules, statistics, plan cache, scenario evidence, compatibility reexports | Split contract/runtime/store/evidence responsibilities without breaking catalog publication recovery. Provisional `andromeda-procedure-store` does not replace catalog runtime ownership yet. |
| `andromeda-srpl` | Facade, binder, lowering, optimizer, interpreter, DefinitionBatch bridge, resolver | Move parser/model/compiler-core work only with SRPL owner tests and facade compatibility. |
| `andromeda-exec` | Admission, dispatch, Procedure runtime, result stream, retry, SRPL adapters, vertical slice | Preserve admission-before-transaction and contract-first invocation during every split. |
| `andromeda-tx` | Transaction state, commit log, MVCC, locking, GC, savepoints, WAL adapter | Do not split C5 transaction behavior until durable evidence and recovery gates exist. |
| `andromeda-storage` | Page, heap, index, buffer pool, manifest, recovery, backup, restore, HA/DR, storage WAL compatibility | Do not move C5 storage behavior while the packet is dirty; separate owner evidence from integration evidence. |
| `andromeda-proto` | Protobuf generated code, manifests, payloads, structured projections, validation | Split wire/generated concerns only with deterministic schema and compatibility tests. |
| `andromeda-quic` | Transport runtime, compatibility reexports, route admission, optional Quinn behavior | Keep runtime-free protocol contracts outside Quinn-backed runtime code. |
| `andromeda-observe` | Events, audit, durable audit, query, sinks, principal binding | Keep observability as evidence, not database truth. |
| `andromeda-bench` | Workloads, harness, regression detection, ScenarioEvidence | Keep benchmark output advisory and version-bound. |
| `andromeda-cli` | Operator entry points for audit, benchmark, backup, restore, HADR, catalog, vertical paths | Keep CLI as a facade over real runtime or explicit dry-run behavior; do not hide mocks behind production wording. |

## Acceptance Gates

| Gate | Required before | Command or evidence class |
|---|---|---|
| Path ownership | Any ledger item becomes an implementation packet | File owner assignment, path-specific diff, no broad staging or unrelated revert. |
| Naming decision | Creating a crate whose target name differs from current branch naming | ADR or same-packet governance note explaining final name, compatibility path, and exit criteria. |
| Topology | Any crate creation or dependency change | `cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture` and `cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture` after a clean candidate exists. |
| Runtime-free contract | Contract, protocol, security-contract, SRPL model crates | Dependency allowlists prove no runtime store, Quinn, GPU, benchmark, SQL, runtime JSON default, or native-layout persistence drift. |
| Facade compatibility | Any move out of a broad current owner | Direct owner tests plus old-import compatibility tests before deleting reexports. |
| Persistent or network bytes | Codec, WAL, page, manifest, catalog WAL, RPC frame, Protobuf wire, storage segment crates | Explicit codecs, format identity, version fields, length bounds, checksum or digest coverage, unknown-version rejection, and golden vectors. |
| C5 extraction | WAL, transaction, storage, recovery, catalog publication, security-critical admission, backup, restore, HA/DR | Crash/recovery matrix, property or fuzz tests, durable evidence proof, threat-model or permission proof where relevant. |
| Adaptive behavior | Statistics, optimizer, plan cache, Maps, benchmarks, ScenarioEvidence | Bounded candidate set, version binding, DecisionTrace, disable path, expiration or staleness policy, no-authoritative-evidence rule. |
| Fuzz registry | Any fuzz target, corpus, or sustained fuzz evidence claim | Keep canonical harnesses, registry, and deterministic corpus metadata under `fuzz/`; treat `tests/fuzzing/` as an index and planning surface only. |
| GPU | `andromeda-gpu`, `andromeda-vector`, GPU use in hardware or analytics paths | Optional feature, CPU fallback, kill switch, device/kernel trace, validation evidence, no-C5-import scan. |
| Release promotion | Any claim that a phase is accepted | Clean candidate, exact command log, commit SHA, toolchain, pass/fail status, skipped tests, unresolved gaps, and ADR/DEC references. |

## Validation

This ledger was validated by targeted repository inspection:

- Root `Cargo.toml` and `crates/` were inspected to confirm the current
  32-crate branch reality before any additional scaffold packet.
- The external roadmap referenced by
  `documentations/architecture/WORKSPACE_RESTRUCTURE_BASELINE_2026.md` was
  inspected for target crate names and roadmap steps.
- The target names were compared by exact crate name against current
  directories under `crates/`.
- `docs/adr/ADR-0011-workspace-crate-boundaries.md` was inspected for rings,
  temporary exceptions, and durable-kernel extraction constraints.
- `documentations/ROADMAP_RESTRUCTURE_STATUS_2026_05_08.md` and
  `documentations/implementation/worktree-packaging-plan-2026-05-08.md` were
  inspected for dirty-worktree risk and packet ordering.
- `fuzz/README.md`, `tests/fuzzing/targets.toml`, and `fuzz/VALIDATION_MATRIX.md` were
  inspected as the canonical fuzz workspace and registry surface.
- Documentation path mapping between `docs/` and `documentations/` was left to
  the separate documentation mapping owner.

No Rust build, Cargo test, clippy, audit, deny, or Codex tooling validation was
run because this task changes only standalone implementation documentation.

## Troubleshooting

| Symptom | Corrective action |
|---|---|
| A missing exact target name already has a current partial equivalent. | Record the current equivalent and require an ADR or governance note before renaming or creating a duplicate crate. |
| A future worker wants to create all missing crates in one packet. | Reject the packet. Create only the smallest owner-scoped crate needed for the current validated boundary. |
| A future worker wants to split `andromeda-storage`, `andromeda-tx`, or `andromeda-wal` while the packet is dirty. | Stop and reconcile the packet first. Add behavior locks and crash/recovery tests before C5 extraction. |
| A future worker treats `andromeda-security-contract` as IAM runtime. | Correct the scope to runtime-free vocabulary and route IAM runtime work through a later security/admission/audit packet. |
| A future worker treats `andromeda-rpc-protocol` as QUIC runtime. | Correct the scope to runtime-free protocol contracts and keep Quinn behavior in the transport runtime layer. |
| A future worker introduces GPU imports from durable-kernel or security-critical crates. | Block the change until topology tests and GPU exclusion policy prove the critical path remains GPU-free. |
| A future worker moves canonical fuzz ownership into `tests/fuzzing/`. | Keep harnesses, registry, corpus manifest, and generator under `fuzz/`; use `tests/fuzzing/` only as a validation index. |
| A future worker asks this ledger to normalize `docs/` and `documentations/` references. | Defer to the documentation path-mapping owner and update only the owned roadmap-status facts here. |

## References

- `AGENTS.md`
- `Cargo.toml`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `docs/adr/ADR-0012-quic-rpc-no-grpc.md`
- `documentations/ROADMAP_RESTRUCTURE_STATUS_2026_05_08.md`
- `documentations/architecture/WORKSPACE_RESTRUCTURE_BASELINE_2026.md`
- `documentations/implementation/roadmap-execution-plan-2026-05-08.md`
- `documentations/implementation/worktree-packaging-plan-2026-05-08.md`
- `documentations/testing/step-11-validation-matrix.md`
- `documentations/governance/adr-backlog-2026-05-08.md`
- `fuzz/README.md`
- `tests/fuzzing/targets.toml`
- `fuzz/VALIDATION_MATRIX.md`
- `C:/Users/Arius/Desktop/andromeda_roadmap_restructuration_workspace_crates_engines_2026.md`
