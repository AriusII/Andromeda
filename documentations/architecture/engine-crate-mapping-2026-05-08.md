# Engine Crate Mapping - 2026-05-08

## Purpose

Map Andromeda macro-engines and transverse planes to Rust crates for the active
workspace restructure.

This document is the operational companion to
`docs/adr/ADR-0018-engine-crate-mapping-policy.md`. It helps future packets
choose the owner crate, compatibility facade, dependency direction, and
validation gate before moving code or changing manifests.

## Scope

This mapping covers the local Andromeda workspace on 2026-05-08 on branch
`codex/workspace-crate-restructure`.

The worktree is dirty. Existing architecture ledgers describe an older
26-crate Step 0 snapshot, while the current root `Cargo.toml` and
`cargo metadata` output are ahead of that snapshot. The current local evidence
observed for this document is:

- `git status --short --branch` shows many modified, added, deleted, and
  untracked files outside this documentation packet.
- The root `Cargo.toml` lists additional workspace crates such as
  `andromeda-codec`, `andromeda-maps`, `andromeda-policy`,
  `andromeda-procedure-store`, and `andromeda-resource`.
- `cargo metadata --no-deps --format-version 1` succeeds locally and reports
  `andromeda-srpl-lexer` as a local workspace package through SRPL path
  dependencies.

The 32-crate count covers root workspace members under `crates/` before any
additional target-crate scaffolds. The canonical fuzz workspace remains under
`fuzz/` and is not counted as an engine crate.

Use this document as dirty-branch planning evidence only. Do not use it as
release readiness evidence.

## Non-goals

- Do not create, delete, rename, move, stage, unstage, or commit any crate.
- Do not edit Cargo manifests, Rust sources, tests, hooks, CI, generated files,
  or governance indexes.
- Do not claim that provisional crates own production behavior before direct
  owner tests exist.
- Do not override ADR-0011 dependency rings.
- Do not accept C5 extraction without behavior locks and crash/recovery or
  threat-model evidence.
- Do not let compatibility facades hide canonical ownership.
- Do not move or redefine canonical fuzz ownership; harnesses, target registry,
  corpus manifest, and generator remain under `fuzz/`.
- Do not normalize `docs/` and `documentations/` path mapping in this mapping
  packet. That clarification is owned by a separate documentation packet.

## Prerequisites

Before changing crate ownership, read:

- `AGENTS.md`
- `docs/AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `docs/adr/ADR-0018-engine-crate-mapping-policy.md`
- `documentations/architecture/dependency-edge-matrix-2026-05-08.md`
- `documentations/architecture/module-criticality-c0-c5-2026-05-08.md`
- `documentations/architecture/module-inventory-2026-05-08.md`
- `documentations/architecture/reexport-migration-ledger-2026-05-08.md`
- `documentations/implementation/target-crate-gap-ledger-2026-05-08.md`

Before accepting manifest changes, refresh local metadata:

```powershell
cargo metadata --no-deps --format-version 1
```

## Procedure

Use this process for a crate split or dependency review.

1. Identify the macro-engine row in this document.
2. Check whether the crate is a canonical owner, temporary facade, broad mixed
   owner, or provisional local shell.
3. Select the smallest owner-scoped packet.
4. Preserve old imports through compatibility facades until caller migration is
   proven.
5. Add direct owner tests before claiming canonical ownership.
6. Run topology validation for dependency or manifest changes.
7. For C4 or C5 behavior, run the subsystem-specific tests named in the row.
8. Record dirty-worktree limits and skipped gates in the packet summary.

## Current Workspace Evidence

The current dirty branch contains more crate surfaces than the older Step 0
architecture ledgers. Use these categories when reviewing ownership.

| Category | Crates | Planning meaning |
| --- | --- | --- |
| Foundation and low-level vocabulary | `andromeda-error`, `andromeda-digest`, `andromeda-types`, `andromeda-time`, `andromeda-hardware`, `andromeda-codec`, `andromeda-policy`, `andromeda-resource`, `andromeda-core` | `andromeda-core` is still a temporary facade. The new `codec`, `policy`, and `resource` crates are provisional until code and tests prove final ownership. |
| Contract and protocol contracts | `andromeda-contract`, `andromeda-structured-object`, `andromeda-security-contract`, `andromeda-proto`, `andromeda-rpc-protocol` | Keep runtime-free contracts separate from runtime stores, Quinn, execution, storage, WAL, benchmark, GPU, SQL, and runtime JSON default. |
| SRPL language model and facade | `andromeda-srpl-diagnostics`, `andromeda-srpl-cardinality`, `andromeda-srpl-ast`, `andromeda-srpl-lexer`, `andromeda-srpl-parser`, `andromeda-srpl-ir`, `andromeda-srpl` | Model crates must stay catalog-store-free. `andromeda-srpl` remains the broad compiler and bridge facade. |
| Durable kernel | `andromeda-wal`, `andromeda-storage`, `andromeda-tx` | Treat as C5. Do not split durable behavior without owner tests, compatibility tests, and crash/recovery evidence. |
| Catalog and adaptive metadata | `andromeda-catalog`, `andromeda-procedure-store`, `andromeda-maps` | `andromeda-catalog` remains broad. `andromeda-procedure-store` and `andromeda-maps` are provisional local shells unless later packets move behavior and tests into them. |
| Execution and transport | `andromeda-exec`, `andromeda-quic` | `andromeda-exec` still has a temporary QUIC bridge. `andromeda-quic` is the concrete transport runtime and must not own Procedure semantics or storage truth. |
| Observability, tools, and evidence | `andromeda-observe`, `andromeda-cli`, `andromeda-bench` | Observability and benchmark evidence are not database truth. CLI and benchmark crates must not become production dependencies. |

## Roadmap Status Consolidation

Use these read-only findings when interpreting the map.

| Finding | Mapping implication |
| --- | --- |
| The current workspace has 32 root members under `crates/` before any additional scaffold packets. | Treat new target-named crates as branch output, not accepted ownership. Future crate creation still needs an owner statement, topology gates, and path-local validation. |
| Most roadmap phases remain partial. | Do not promote a phase because one owner crate or specification exists; each phase still needs clean candidate tests and retained evidence. |
| `andromeda-maps` and `andromeda-procedure-store` are provisional. | Keep current durable or runtime behavior attributed to `andromeda-catalog`, `andromeda-exec`, storage, or other existing owners until direct owner tests and integration gates move it. |
| Fuzz remains canonical under `fuzz/`. | Use `fuzz/targets.toml`, `fuzz/corpus/manifest.toml`, and `fuzz/VALIDATION_MATRIX.md` for harness and corpus authority; use `tests/fuzzing/` only as an index and validation-planning surface. |
| Documentation path mapping is being clarified elsewhere. | Preserve existing `docs/` and `documentations/` references in this packet unless they are required for one of the owned files. |
| Release blockers remain. | Dirty worktree state, missing C5 crash/recovery evidence, sustained fuzz gaps, Miri/Loom gaps, and release evidence gaps still block readiness claims. |

## Macro-Engine Map

| Macro-engine or plane | Current owner crates | Current facade or debt | Not owned here | Minimum validation before ownership changes |
| --- | --- | --- | --- | --- |
| Core Engine | `andromeda-error`, `andromeda-digest`, `andromeda-types`, `andromeda-time`, `andromeda-hardware`; provisional `andromeda-codec`, `andromeda-policy`, `andromeda-resource`. | `andromeda-core` still reexports foundation types and owns current principal identity. | Catalog store, execution, WAL, storage, transport runtime, benchmark, GPU runtime. | R0 dependency allowlist, facade compatibility, caller migration, topology test. |
| Catalog and Contract Engine | `andromeda-contract` for contract-safe Procedure and catalog descriptors. `andromeda-catalog` for current catalog store, DefinitionBatch, publication, Procedure Store, statistics metadata, plan-cache identity, scenario evidence, and catalog WAL integration. | `andromeda-catalog` is both broad owner and contract facade. `andromeda-procedure-store` is provisional. | Execution dispatch, QUIC runtime, WAL physical ownership, storage truth, IAM runtime. | Contract hash tests, catalog compatibility tests, DefinitionBatch dry-run and publication tests, catalog WAL replay tests, topology test. |
| SRPL Compiler | `andromeda-srpl-diagnostics`, `andromeda-srpl-cardinality`, `andromeda-srpl-ast`, `andromeda-srpl-lexer`, `andromeda-srpl-parser`, `andromeda-srpl-ir` for model and syntax responsibilities. | `andromeda-srpl` remains the facade for binder, lowering, optimizer, interpreter, DefinitionBatch bridge, procedure resolver, and historical imports. | Catalog store ownership, execution runtime, storage truth, QUIC, benchmark, GPU. | Parser/model crate tests, catalog-store-free topology gates, SRPL facade compatibility, DefinitionBatch compatibility, fuzz/property where grammar or parsing changes. |
| Protocol and RPC Contract Plane | `andromeda-rpc-protocol` for runtime-free frame and stream contracts. `andromeda-proto` for generated schemas, typed payloads, envelope validation, manifest projection, and completion projection. | `andromeda-quic` reexports selected runtime-free protocol contracts. `andromeda-proto` reexports StructuredObject contracts. | Concrete QUIC sessions, Procedure semantics, storage truth, authorization policy. | No-gRPC and no-runtime-JSON scans, malformed frame tests, deterministic protobuf tests, metadata-before-payload tests, compatibility tests. |
| Network Surface Layer | `andromeda-quic` for concrete QUIC runtime behavior, streams, sessions, transport state, backpressure transport, feature-gated Quinn/Rustls/Tokio. | Runtime-free protocol reexports remain temporary. | Procedure semantics, WAL authority, catalog publication, IAM policy ownership, storage truth. | Surface-separation tests, mTLS/admission tests, route authorization tests, malformed frame tests, feature-gated runtime tests. |
| Security and IAM Plane | `andromeda-security-contract` for runtime-free vocabulary. `andromeda-core` for current principal identity facade. `andromeda-quic`, `andromeda-exec`, and `andromeda-observe` participate in admission and audit paths. | No dedicated IAM runtime crate is accepted yet. `andromeda-core` principal facade remains temporary. | Mutable IAM stores, revocation stores, policy runtime, TLS runtime, catalog store, WAL, storage, recovery in the vocabulary crate. | Permission-denial tests, wrong-surface tests, no-transaction-created-on-denial tests, audit evidence tests, threat-model review, topology allowlist. |
| Execution Engine | `andromeda-exec` for admission integration, dispatch, invocation, local runtime, result stream, retry, services, traces, vertical slice, and transaction orchestration. | Direct `andromeda-exec` to `andromeda-quic` edge remains temporary bridge debt. | Catalog contract ownership, SRPL parser ownership, WAL/storage/transaction canonical ownership, transport runtime ownership. | Admission-before-transaction, contract binding, transaction lifecycle, ResultStream metadata-before-payload, audit completion validation, bridge exit plan. |
| Transaction Kernel | `andromeda-tx` for transaction state, MVCC, locks, savepoints, commit log, WAL adapter, GC, and transaction evidence. | Root and `mvcc` reexports are compatibility paths for future splits. | WAL physical bytes, page storage, QUIC, SRPL, catalog store, benchmark, GPU. | Durable commit evidence, rollback evidence, MVCC isolation, lock and savepoint tests, WAL replay, crash/recovery tests before C5 movement. |
| WAL and Storage Engine | `andromeda-wal` for pure WAL primitives, LSNs, records, codecs, segment descriptors, record bounds, durability fence helpers, and FileWal byte contracts. `andromeda-storage` for pages, heap, B+Tree, buffer pool, disk manager, manifest, recovery, backup, restore, HA/DR, and storage integration. | `andromeda-storage` reexports WAL and FileWal paths. It remains a broad C5 owner and compatibility facade. | SQL surface, SRPL model, execution dispatch, QUIC runtime, benchmark truth, GPU output. | WAL owner tests, storage integration tests, byte-format golden vectors, property/fuzz tests, WAL-before-page-flush tests, manifest recovery, backup/restore/PITR drills, crash/recovery matrix. |
| Observability and Forensic Plane | `andromeda-observe` for traces, events, durable audit evidence, query surfaces, restore trace, principal binding evidence, and exporters. | Dev-only storage test harness edges are not production ownership. | Database truth, commit authority, recovery truth, authorization policy, benchmark authority. | Audit family tests, durable sink and corruption tests, trace correlation tests, evidence-not-truth review, forensic startup/report tests where relevant. |
| Statistics and Optimizer Plane | Current statistics and plan-cache identity live in `andromeda-catalog`; SRPL optimizer scaffolding lives in `andromeda-srpl`; benchmark evidence lives in `andromeda-bench`. | Future `andromeda-statistics`, `andromeda-optimizer`, and `andromeda-plan-cache` crates are not accepted by this file. | Durable truth, forced plan decisions from benchmark output, GPU authority. | StatsVersion publication validation, bounded PlanClass tests, DecisionTrace tests, stale evidence rejection, disablement and fallback checks. |
| Procedure Store | Current Procedure Store modules live in `andromeda-catalog`; execution emits or consumes related runtime evidence through `andromeda-exec` and `andromeda-observe`. | `andromeda-procedure-store` exists in the dirty branch but is provisional until code and tests move. | Catalog contract ownership, durable WAL ownership, ResultStream protocol ownership. | Procedure Store runtime record tests, invocation correlation tests, compatibility facade, catalog publication and recovery tests if durable state moves. |
| Predictive Evidence and Benchmark Engine | `andromeda-bench` for benchmark harnesses, workload evidence, regression detection, and scenario-boundary checks. `andromeda-catalog` contains scenario evidence model paths. | CLI exposes benchmark commands as operator tooling. | Optimizer authority, commit/recovery truth, catalog publication truth. | Advisory-only tests, expiration/staleness tests, bounded score tests, regression evidence tests, no production dependency on R5 crates. |
| Internal Analytical Plane and Maps | `andromeda-maps` is provisional. Current Map/statistics-adjacent descriptors live in catalog and storage documentation or broad owners. | No accepted runtime analytical engine split exists in this packet. | Commit, WAL, rollback, recovery, MVCC short visibility, catalog publication truth, authorization. | Grain and summarizability review, refresh policy tests, StatsVersion/CatalogVersion binding, CPU path first, no-GPU-critical-path scan. |
| Hardware, SIMD, and Future GPU | `andromeda-hardware` owns hardware descriptors and GPU exclusion vocabulary. Future GPU/SIMD/vector crates are not accepted runtime owners. | GPU runtime is future optional work only. | Commit, WAL, rollback, recovery, MVCC short visibility, catalog publication, Procedure admission, authorization, security-critical paths. | CPU fallback, disablement, device/kernel trace fields, validation status, fallback reason, topology scan proving no critical-path imports. |
| Administration, Backup, Restore, and HA/DR | `andromeda-cli` for operator commands. `andromeda-storage` for current backup, restore, PITR, HADR durable logic. `andromeda-quic` for transport surfaces where present. `andromeda-observe` for forensic evidence. | CLI is a tool facade and must not hide production readiness gaps. | Application Surface, ad hoc SQL, benchmark authority. | Admin/Application/Cluster surface separation, quorum/fencing, WAL shipping, PITR exact-LSN tests, restore drills, forensic reports, audit evidence. |

## Allowed Dependency Direction

Use ADR-0011 rings as the default direction.

| Source | May depend on | Must not depend on |
| --- | --- | --- |
| R0 foundation | R0 only, with documented leaf or facade exceptions. | Engine runtime, catalog store, execution, storage, WAL, transaction, transport runtime, benchmark, analytics, GPU runtime, SQL, gRPC, runtime JSON default, native-layout persistence. |
| R1 contracts, protocol contracts, SRPL model | R0 and lower-risk R1 contract-safe crates. | Runtime stores, Quinn/TLS runtime, async runtime, execution, storage, WAL, recovery, benchmark, analytics, GPU, SQL, gRPC, runtime JSON default, native-layout persistence. |
| R2 durable kernel | R0, runtime-free R1 contracts, WAL-safe contracts, typed observability evidence when needed. | SRPL parser/model, catalog store implementations except explicitly owned catalog durability paths, execution, protocol runtime, QUIC runtime, benchmark, analytics, GPU, SQL, gRPC, runtime JSON default, native-layout persistence. |
| R3 execution | R0, R1 contracts, R2 durable APIs, typed observability. | Becoming a dependency of R0/R1 model crates, WAL, storage, transaction owners, or protocol-contract crates. |
| R4 transport runtime | R0, runtime-free protocol/security contracts, typed observability. | Storage truth, WAL authority, Procedure semantics, catalog publication, IAM policy ownership. |
| R5 tools and benchmarks | Lower rings for operator commands, tests, benchmarks, and diagnostics. | Production crates depending on R5 tooling or benchmark output as authority. |

## Temporary Facades

| Facade | Canonical owner | Allowed role | Exit gate |
| --- | --- | --- | --- |
| `andromeda-core` | R0 foundation crates; future principal/security owner. | Preserve historical imports and current principal identity while callers migrate. | Direct foundation imports, principal owner decision, facade compatibility, topology allowlist. |
| `andromeda-catalog` contract paths | `andromeda-contract`. | Preserve contract imports while catalog keeps store/publication ownership. | Caller migration, contract facade tests, catalog keeps only catalog runtime ownership. |
| `andromeda-proto::structured` | `andromeda-structured-object`. | Preserve StructuredObject imports while proto owns schema and projection. | Caller migration, structured-object owner tests, proto compatibility tests. |
| `andromeda-quic` frame and stream paths | `andromeda-rpc-protocol`. | Preserve runtime-free protocol imports through transport crate. | Caller migration to protocol crate, frame/stream owner tests, QUIC compatibility tests. |
| `andromeda-srpl` root and compiler paths | Extracted SRPL model crates plus future binder/lowering/bridge owners. | Preserve historical compiler and bridge imports. | Model caller migration, dedicated bridge owner, parser/model topology gates. |
| `andromeda-storage` WAL and FileWal paths | `andromeda-wal`. | Preserve storage-era WAL imports. | Direct WAL caller migration, WAL owner tests, storage compatibility and recovery integration tests. |
| `andromeda-tx` root and `mvcc` paths | Current `andromeda-tx` until future transaction split crates are accepted. | Preserve transaction imports during future splits. | Future owner crates, transaction compatibility tests, durable commit and replay gates. |

## Extraction Gates

Use these gates before moving behavior out of a broad owner.

| Gate | Applies to | Required evidence |
| --- | --- | --- |
| Ownership statement | Every split or new crate | ADR, DEC, or same-packet governance note naming canonical owner, facade, dependency ring, and exit criteria. |
| Dirty-worktree containment | Every packet | Path-specific diff and status; no broad staging; no unrelated revert; no contradictory staged/unstaged state in owned files. |
| Topology | Manifest or dependency changes | `cargo metadata --no-deps --format-version 1`; `cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture`; orphan/dependency invariant tests where applicable. |
| Runtime-free allowlist | R0/R1 contracts, protocol, security, SRPL model | Dependency allowlist proving no runtime stores, Quinn/TLS runtime, async runtime, storage, WAL, recovery, benchmark, GPU, SQL, gRPC, runtime JSON default, or native-layout persistence. |
| Facade compatibility | Any moved public import | Old-import tests plus direct owner tests before facade removal. |
| Persistent bytes | WAL, page, heap, B+Tree, manifest, backup, recovery, catalog WAL | Explicit codec, format identity, endian policy, version fields, length bounds, checksum/digest, golden vectors, malformed-input rejection. |
| Network bytes | RPC frames, protobuf payloads, envelopes, completions | Explicit frame codec or schema projection, no gRPC, no runtime JSON default, malformed frame tests, metadata-before-payload tests. |
| C5 durable behavior | WAL, storage, transaction, recovery, catalog publication, backup, restore, HA/DR | Owner tests, integration tests, property/fuzz tests, crash/recovery matrix, replay rejection, durable visibility evidence. |
| Security-critical behavior | Admission, IAM, mTLS, permissions, surface routing, audit | Permission denial, wrong surface, disabled principal, no transaction on rejection, audit evidence, threat model. |
| Adaptive behavior | Statistics, optimizer, plan cache, Maps, ScenarioEvidence, benchmark | Bounded candidates, version binding, DecisionTrace, disablement, stale evidence rejection, advisory-only evidence proof. |
| Fuzz ownership | Parser, codec, persistent-byte, protocol, ResultStream, and security-admission fuzz evidence | Canonical harnesses and corpus metadata stay under `fuzz/`; sustained fuzz run evidence must record target, corpus hash, duration, commit, sanitizer mode, crash count, and artifacts. |
| GPU or SIMD | Future optional hardware acceleration | CPU fallback, disablement, trace fields, no critical-path import scan, failure does not alter contractual or recovered results. |

## Validation

This documentation-only mapping was validated with targeted inspection:

```powershell
Get-Content -Raw AGENTS.md
Get-Content -Raw docs\AGENTS.md
git status --short --branch
Get-Content -Raw Cargo.toml
cargo metadata --no-deps --format-version 1
rg -n "macro-engine|macro engine|engine mapping|crate mapping|R0|R1|R2|R3|R4|R5" documentations docs crates\README.md Cargo.toml
rg -n "fuzz/|targets.toml|VALIDATION_MATRIX|corpus/manifest" fuzz tests documentations -g "*.md" -g "*.toml"
```

No Rust build, clippy, nextest, audit, deny, fuzz, Miri, crash/recovery, or
Codex tooling validation is required for this documentation-only change.

Future source or manifest changes must run the gates listed in the relevant
macro-engine row and in `docs/adr/ADR-0018-engine-crate-mapping-policy.md`.

## Risks

- Existing architecture ledgers may conflict with the dirty local crate count
  until a later owned documentation packet refreshes them.
- Provisional local crates can be mistaken for accepted owners without tests.
- Fuzz registry ownership can drift if planning documents under `tests/` are
  mistaken for canonical harness or corpus locations.
- Documentation path references can drift while `docs/` and `documentations/`
  mapping is clarified by the separate documentation owner.
- Broad owners can continue to grow if facade exits are not enforced.
- C5 extraction can pass import compatibility while failing owner behavior or
  crash/recovery evidence.
- Tooling and benchmark crates can accidentally become production dependencies
  if topology tests are not maintained.
- Observability, audit, benchmark, GPU, or ScenarioEvidence data can be
  overstated as truth.

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| A worker wants to move code into a crate because the crate already exists. | Require owner tests, dependency allowlist, topology validation, and a mapping update before accepting the move. |
| A broad crate gains an unrelated module. | Classify the module against this map and either move it to the owner or document a temporary facade with exit criteria. |
| A facade defines new canonical behavior. | Move the behavior to the canonical owner and leave a reexport-only facade. |
| `cargo metadata` shows more crates than this document. | Refresh the map in a documentation-owned packet and explain whether the new crate is owner, facade, tool, or provisional. |
| A C5 split lacks crash/recovery evidence. | Block the split until durable behavior tests and recovery gates exist. |
| A benchmark or GPU output is used as acceptance truth. | Reword as advisory evidence and require CPU-backed or durable owner evidence. |
| A worker treats `tests/fuzzing/` as the canonical fuzz workspace. | Redirect to `fuzz/targets.toml`, `fuzz/corpus/manifest.toml`, `fuzz/generators/generate_seed_corpus.py`, and `fuzz/VALIDATION_MATRIX.md`. |
| A worker asks this map to settle `docs/` versus `documentations/`. | Leave path normalization to the dedicated documentation mapping packet and keep this map focused on crate ownership. |

## References

- `AGENTS.md`
- `docs/AGENTS.md`
- `Cargo.toml`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `docs/adr/ADR-0015-wal-commit-visibility.md`
- `docs/adr/ADR-0016-gpu-exclusion-critical-paths.md`
- `docs/adr/ADR-0018-engine-crate-mapping-policy.md`
- `documentations/01_DOCTRINE_LEXIQUE_ARCHITECTURE_CATALOGUE_MODELIZATION.md`
- `documentations/architecture/WORKSPACE_RESTRUCTURE_BASELINE_2026.md`
- `documentations/architecture/dependency-edge-matrix-2026-05-08.md`
- `documentations/architecture/module-criticality-c0-c5-2026-05-08.md`
- `documentations/architecture/module-inventory-2026-05-08.md`
- `documentations/architecture/reexport-migration-ledger-2026-05-08.md`
- `documentations/governance/adr-backlog-2026-05-08.md`
- `documentations/implementation/target-crate-gap-ledger-2026-05-08.md`
- `fuzz/README.md`
- `fuzz/targets.toml`
- `fuzz/VALIDATION_MATRIX.md`
