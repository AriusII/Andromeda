# ADR-0018: Macro-Engine Crate Mapping Policy

## Status

Accepted

## Purpose

Define the canonical mapping from Andromeda macro-engine responsibilities to
Rust crates, dependency rings, temporary compatibility facades, and extraction
gates.

This ADR complements ADR-0011. ADR-0011 defines the workspace dependency rings.
This ADR maps product architecture terms such as Storage Engine, Transaction
Kernel, SRPL Compiler, Catalog and Contract Engine, and Observability Plane to
current crates and future extraction boundaries.

## Scope

This ADR applies to:

- macro-engine ownership claims in architecture documents;
- crate creation, split, rename, and facade retirement proposals;
- dependency direction reviews;
- topology validation and release-gate evidence;
- documentation that maps Andromeda engines and planes to Rust crates.

This ADR covers the current dirty local branch state on
`codex/workspace-crate-restructure`. Existing Step 0 architecture ledgers still
describe a 26-crate snapshot. The current root `Cargo.toml` and local
`cargo metadata --no-deps --format-version 1` output are ahead of those ledgers:
the root manifest lists additional workspace crates, and metadata reports
`andromeda-srpl-lexer` as a local workspace package through SRPL path
dependencies. Treat those counts as dirty-branch evidence, not release
readiness.

## Non-goals

- Do not change Rust code, Cargo manifests, tests, hooks, CI, or generated
  files.
- Do not approve a new runtime behavior only because a crate name exists.
- Do not declare the dirty branch release-ready.
- Do not replace ADR-0011 dependency rings or weaken any temporary exception
  exit criterion.
- Do not authorize application-facing ad hoc SQL, gRPC, runtime JSON as the
  default protocol, or native Rust struct layout as a disk or network format.
- Do not move C5 storage, WAL, recovery, transaction, catalog publication,
  security, backup, restore, or HA/DR behavior without owner tests and
  crash/recovery or threat-model evidence.
- Do not promote RAM, temp storage, GPU output, benchmark output, audit records,
  or ScenarioEvidence into database truth.

## Prerequisites

Before using this ADR for an implementation packet, read:

- `AGENTS.md`
- `docs/AGENTS.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `docs/adr/ADR-0015-wal-commit-visibility.md`
- `docs/adr/ADR-0016-gpu-exclusion-critical-paths.md`
- `crates/README.md`
- `documentations/architecture/dependency-edge-matrix-2026-05-08.md`
- `documentations/architecture/module-criticality-c0-c5-2026-05-08.md`
- `documentations/architecture/module-inventory-2026-05-08.md`
- `documentations/architecture/reexport-migration-ledger-2026-05-08.md`
- `documentations/architecture/engine-crate-mapping-2026-05-08.md`

If the worktree is dirty, capture a path-specific status and avoid release
claims.

## Context

Andromeda architecture is intentionally split into macro-engines and transverse
planes. A macro-engine owns a product responsibility. A crate owns Rust code,
public imports, dependency edges, and tests. Those two maps are related, but
they are not identical.

Current restructuring has already extracted several responsibility-specific
crates, including foundation crates, contract crates, SRPL model crates,
`andromeda-wal`, `andromeda-rpc-protocol`, and
`andromeda-security-contract`. Several broad crates still remain active
temporary facades or mixed owners:

- `andromeda-core`
- `andromeda-catalog`
- `andromeda-srpl`
- `andromeda-exec`
- `andromeda-storage`
- `andromeda-tx`
- `andromeda-quic`
- `andromeda-proto`
- `andromeda-observe`
- `andromeda-bench`
- `andromeda-cli`

The mapping must therefore distinguish:

- canonical owner crates;
- temporary compatibility facades;
- broad current owners that still need later splits;
- shell or provisional crates that exist in the dirty branch but do not yet own
  promoted runtime behavior;
- future target crates that require separate ADR, DEC, or same-packet
  governance before creation or promotion.

## Decision

Andromeda accepts the macro-engine to crate mapping in this ADR as the review
policy for future crate topology work.

### Dependency Rings

Dependencies may point only within the same ring or toward a lower ring unless
an ADR records a bounded exception with an exit criterion.

| Ring | Ownership rule | Current examples |
| --- | --- | --- |
| R0 Foundation | Runtime-free, engine-agnostic primitives, typed errors, identifiers, digests, time, hardware descriptors, policy and resource vocabulary. | `andromeda-error`, `andromeda-digest`, `andromeda-types`, `andromeda-time`, `andromeda-hardware`, provisional `andromeda-codec`, `andromeda-policy`, `andromeda-resource`, temporary `andromeda-core` facade. |
| R1 Contracts, protocol contracts, and language model | Runtime-free Procedure contracts, StructuredObject contracts, security vocabulary, protocol schemas or frame contracts, SRPL syntax and semantic model. | `andromeda-contract`, `andromeda-structured-object`, `andromeda-security-contract`, `andromeda-proto`, `andromeda-rpc-protocol`, `andromeda-srpl-diagnostics`, `andromeda-srpl-cardinality`, `andromeda-srpl-ast`, `andromeda-srpl-lexer`, `andromeda-srpl-parser`, `andromeda-srpl-ir`. |
| R2 Durable kernel and catalog truth | WAL, storage, recovery, transaction state, MVCC, manifest, catalog publication, DefinitionBatch durability, backup, restore, HA/DR durable state. | `andromeda-wal`, `andromeda-storage`, `andromeda-tx`, C5 portions of `andromeda-catalog`; future recovery, page, heap, buffer-pool, manifest, backup, restore, HADR, MVCC, and lock crates. |
| R3 Execution and invocation orchestration | Admission-before-transaction, Procedure invocation, dispatch, result sequencing, retry, transaction orchestration, execution traces. | `andromeda-exec`; future `andromeda-admission`, `andromeda-procedure-runtime`, `andromeda-result-stream`, `andromeda-retry`, `andromeda-execution-trace`. |
| R4 Transport runtime and network surfaces | Concrete QUIC runtime, stream handling, transport sessions, feature-gated Quinn/Rustls/Tokio behavior, Application/Admin/Cluster surface separation. | `andromeda-quic`; future `andromeda-quic-runtime-quinn`. |
| R5 Tools, operators, benchmarks, and advisory evidence | CLI, diagnostics, benchmarks, workload harnesses, regression tracking, offline evidence, release tooling. | `andromeda-cli`, `andromeda-bench`; future benchmark harness, workload, scenario-evidence, regression, and runbook crates. |

Observability is a cross-cutting evidence plane. `andromeda-observe` may be used
by production crates to emit typed post-fact evidence, but observability output
is never database truth and must not become a dependency that decides commit,
rollback, recovery, catalog publication, or authorization.

### Macro-Engine Mapping

| Macro-engine or plane | Canonical current owner or owners | Temporary facades or mixed owners | Allowed dependency posture | Extraction gate |
| --- | --- | --- | --- | --- |
| Core Engine and Foundation Plane | R0 foundation crates: `andromeda-error`, `andromeda-digest`, `andromeda-types`, `andromeda-time`, `andromeda-hardware`; provisional `andromeda-codec`, `andromeda-policy`, `andromeda-resource`. | `andromeda-core` remains a temporary facade and still contains principal identity. | R0 crates may not depend on engine, catalog store, execution, storage, transport runtime, benchmark, GPU runtime, SQL, runtime JSON default, or native-layout persistence dependencies. | Topology allowlist, facade compatibility, and direct caller migration before retiring `andromeda-core`. |
| Catalog and Contract Engine | `andromeda-contract` owns contract-safe Procedure and catalog object descriptors. `andromeda-catalog` owns current catalog store, DefinitionBatch, Procedure Store, publication, statistics metadata, plan-cache identity, and catalog WAL integration. | `andromeda-catalog` reexports contract types and remains broad. `andromeda-procedure-store` is provisional in the dirty branch until implementation and tests prove ownership. | Contract crates depend only on R0. Catalog runtime may depend on contract, digest, types, observe, proto as documented, but must not depend on execution or transport runtime. | Contract-hash golden tests, DefinitionBatch dry-run and publication tests, catalog WAL replay tests, facade compatibility, topology tests. |
| SRPL Compiler | Extracted language-model crates: `andromeda-srpl-diagnostics`, `andromeda-srpl-cardinality`, `andromeda-srpl-ast`, `andromeda-srpl-lexer`, `andromeda-srpl-parser`, `andromeda-srpl-ir`. | `andromeda-srpl` remains the compiler facade and owns binder, lowering, optimizer, interpreter, DefinitionBatch bridge, and resolver work in the current snapshot. | Parser/model crates may depend on R0 and contract-safe identifiers. They must not depend on catalog store, execution, storage, transport, benchmark, analytics, or GPU crates. | Parser/model tests, catalog-store-free topology tests, DefinitionBatch compatibility, old-import facade tests, fuzz or property tests for accepted grammar surfaces. |
| Protocol and RPC Contract Plane | `andromeda-rpc-protocol` owns runtime-free frame and stream contracts. `andromeda-proto` owns typed protocol payloads, generated schemas, manifest projection, and envelope validation. | `andromeda-quic` temporarily reexports frame and stream contracts. `andromeda-proto` temporarily reexports StructuredObject contracts. | Runtime-free protocol crates may depend on R0 and contract-safe R1 only. They must not depend on Quinn, TLS runtime, async runtime, execution, storage, WAL, recovery, benchmark, gRPC, or runtime JSON default. | No-gRPC and no-runtime-JSON scans, frame malformed-input tests, metadata-before-payload tests, deterministic protobuf projection tests, compatibility reexport tests. |
| Network Surface Layer | `andromeda-quic` owns concrete QUIC transport behavior and feature-gated Quinn/Rustls/Tokio runtime behavior. | `andromeda-quic` still carries compatibility reexports from `andromeda-rpc-protocol`. | R4 may depend on R0 and runtime-free protocol/security contracts. It must not own Procedure semantics, WAL authority, storage truth, or authorization policy. | Surface-separation tests, malformed frame tests, mTLS/admission tests, optional runtime feature gates, no Admin/Application/Cluster surface mixing. |
| Security and IAM Plane | `andromeda-security-contract` owns runtime-free security vocabulary. `andromeda-core` currently owns principal identity facade. Admission enforcement is currently integrated through `andromeda-quic`, `andromeda-exec`, and `andromeda-observe`. | `andromeda-core` principal facade is temporary. Future `andromeda-security`, `andromeda-iam`, and `andromeda-audit` crates require separate gates. | Security contract vocabulary stays runtime-free. IAM runtime stores, policy stores, revocation, audit ledgers, TLS runtime, catalog stores, execution, WAL, storage, and recovery must not enter the vocabulary crate. | Permission-denial tests, no-transaction-on-denial tests, audit evidence tests, threat-model review, topology allowlists. |
| Execution Engine | `andromeda-exec` owns current Procedure execution orchestration, admission integration, local runtime, result sequencing, retry, vertical slice, and transaction orchestration. | Direct `andromeda-exec` to `andromeda-quic` bridge is temporary debt from ADR-0011. | R3 may depend on R0, R1 contracts, R2 durable kernel APIs, and typed observability. It must not be a dependency of catalog contracts, SRPL parser/model crates, WAL, storage, or transaction owners. | Admission-before-transaction tests, Procedure contract binding tests, ResultStream metadata-before-payload tests, transaction lifecycle tests, bridge exit plan. |
| Transaction Kernel | `andromeda-tx` owns current transaction state, MVCC, locks, savepoints, commit log, WAL adapter, and transaction evidence. | `andromeda-tx` root and module reexports are temporary compatibility paths for future transaction, MVCC, locking, savepoint, and transaction-log splits. | C5 transaction crates may depend on R0, WAL-safe contracts, durable kernel APIs, and typed observability evidence. They must not depend on SRPL, catalog store, execution, QUIC, benchmark, GPU, SQL, or native-layout serialization dependencies. | Durable commit evidence, rollback evidence, visible-commit tests, MVCC isolation tests, replay tests, lock tests, crash/recovery gates before C5 extraction. |
| WAL, Storage, and Recovery Engine | `andromeda-wal` owns pure WAL primitives and physical FileWal byte contracts. `andromeda-storage` owns current pages, heap, B+Tree, buffer pool, disk manager, manifest, recovery, backup, restore, HA/DR, and storage integration. | `andromeda-storage` reexports WAL and FileWal compatibility paths. It remains the largest broad C5 owner. | Durable-kernel crates must not depend on SRPL model crates, catalog store implementations, execution, protocol runtime, QUIC runtime, benchmark, analytics, GPU, SQL, runtime JSON default, or native-layout persistence. | Owner tests plus integration tests, byte-format golden vectors, property/fuzz tests, WAL-before-page-flush tests, manifest switch recovery, backup/restore/PITR drills, crash/recovery matrix. |
| Observability and Forensic Plane | `andromeda-observe` owns typed traces, audit evidence, durable audit journal evidence, query surfaces, restore traces, and principal binding evidence. | Storage dev-dependencies in observe tests are test harness edges only. | Observability may receive evidence from critical paths but does not become truth and must not block commit, recovery, or authorization unless a security-specific gate says so. | Audit family tests, durable sink and corruption tests, trace correlation tests, evidence-not-truth wording and review gates. |
| Statistics, Optimizer, Procedure Store, and Adaptive Plane | Current statistics, plan-cache identity, Procedure Store, and scenario evidence live primarily in `andromeda-catalog`; benchmark evidence lives in `andromeda-bench`; provisional `andromeda-procedure-store` and `andromeda-maps` exist in the dirty branch. | `andromeda-catalog` is broad and should not be treated as the final optimizer/statistics/procedure-store split. | Adaptive outputs must be bounded, versioned, observable, explainable, disableable, and never sole authority for durable truth. | StatsVersion publication tests, bounded PlanClass tests, DecisionTrace tests, stale evidence rejection, no-authoritative-ScenarioEvidence tests, topology checks. |
| Internal Analytical Plane, Maps, and Future GPU | `andromeda-hardware` owns hardware and GPU exclusion vocabulary. `andromeda-maps` is provisional in the dirty branch. `andromeda-bench` owns advisory benchmark evidence. | Future GPU, SIMD, vector, analytics, columnar, and maps crates require optional boundaries and CPU fallback. | GPU and advisory analytics must not enter commit, WAL, rollback, recovery, MVCC short visibility, catalog publication, Procedure admission, authorization, or security-critical paths. | CPU fallback tests, disablement tests, topology import scans, trace fields, validation status and fallback reason, no-C5-import proof. |
| Administration, Operations, Backup, Restore, and HA/DR | `andromeda-cli` owns operator entry points. `andromeda-storage` owns current backup, restore, PITR, HA/DR durable logic. `andromeda-quic` owns HADR transport surfaces where present. `andromeda-observe` owns forensic evidence. | CLI is a tool facade, not an engine truth owner. Future admin, forensic, backup, restore, and HADR crates require separate gates. | Administration and HA/DR capabilities must not be exposed through the Application Surface. R5 tools may call downward, but production crates must not depend on tools. | Surface separation, quorum/fencing tests, PITR exact-LSN tests, restore drills, forensic startup reports, audit and operator command tests. |

### Temporary Facade Policy

A facade may remain only when it preserves public import compatibility during a
bounded migration. A facade must not become a new owner for behavior that
belongs to another crate.

Temporary facade paths include:

- `andromeda-core` for foundation reexports and current principal identity.
- `andromeda-catalog` for contract reexports while `andromeda-contract` is the
  contract owner.
- `andromeda-proto` for StructuredObject reexports while
  `andromeda-structured-object` is the contract owner.
- `andromeda-quic` for RPC frame, stream, and selected backpressure reexports
  while `andromeda-rpc-protocol` is the runtime-free owner.
- `andromeda-srpl` for historical compiler imports and catalog-facing SRPL
  bridge modules.
- `andromeda-storage` for WAL and FileWal import compatibility while
  `andromeda-wal` owns pure WAL and physical FileWal byte contracts.
- `andromeda-tx` root and module facades for future transaction, MVCC, lock,
  savepoint, and transaction-log splits.

Every facade has the same exit criteria:

1. A canonical owner crate exposes the needed public symbols.
2. Downstream callers migrate to the canonical owner or a documented stable
   domain path.
3. Old-import compatibility tests pass until removal is explicitly approved.
4. Owner tests prove behavior in the canonical crate.
5. Topology tests prove the facade did not add new forbidden dependencies.
6. The facade stops defining new canonical behavior for the moved surface.

### Forbidden Edges

The following edges are rejected unless a later ADR explicitly revises this
policy:

- Application Surface to Administration Plane or Cluster Engine capabilities.
- R0 foundation crates to engine runtime, storage, transaction, catalog store,
  execution, transport runtime, benchmark, analytics, or GPU runtime crates.
- Contract, SRPL model, protocol-contract, or security-vocabulary crates to
  runtime stores, Quinn/TLS runtime, async runtime, storage, WAL, recovery,
  execution, benchmark, GPU, SQL, gRPC, runtime JSON default, or native-layout
  persistence dependencies.
- Durable-kernel crates to SRPL parser/model crates, catalog store
  implementations, execution crates, QUIC runtime, protocol runtime, benchmark,
  analytics, GPU, SQL, gRPC, runtime JSON default, or native-layout persistence.
- Execution crates as dependencies of catalog contracts, SRPL model crates, WAL,
  storage, transaction, or protocol-contract crates.
- Transport runtime crates owning Procedure semantics, storage truth, WAL
  authority, authorization policy, or catalog publication.
- Benchmark, ScenarioEvidence, GPU, RAM, temp storage, ResultStream metadata,
  or audit records as database truth.

## Procedure

Use this procedure for any future crate split, new crate, dependency change, or
facade retirement.

1. Identify the macro-engine or plane and the highest criticality touched.
2. Identify the current canonical owner, temporary facade, and intended target
   owner.
3. Confirm that the packet is owner-scoped and that staged and unstaged content
   for the touched paths are coherent.
4. Update the architecture map, ADR, DEC, or topology test in the same packet
   when the ownership claim changes.
5. Preserve public imports through a facade until direct caller migration and
   compatibility tests prove it is safe to remove.
6. Add direct owner tests before treating a moved surface as owned by the target
   crate.
7. For R0 and R1 runtime-free crates, enforce strict dependency allowlists.
8. For persistent or network bytes, require explicit codecs, version or format
   identity, length bounds, checksum or digest coverage, golden vectors, and
   malformed-input rejection.
9. For C4 or C5 paths, add the owning subsystem tests plus crash/recovery,
   property, fuzz, Miri, threat-model, or audit evidence as applicable.
10. Run `cargo metadata --no-deps --format-version 1` and the dependency
    topology test before accepting manifest changes.
11. Record skipped tests and dirty-worktree constraints explicitly.

## Validation

This documentation-only ADR is validated by repository inspection and
consistency checks. It does not require a Rust build because it does not change
code, Cargo manifests, tests, hooks, generated sources, or runtime behavior.

For this ADR itself, validation consists of:

- confirming that it preserves Andromeda invariants from `AGENTS.md`;
- confirming that it does not weaken ADR-0011 dependency rings;
- confirming that it keeps GPU, benchmark, RAM, temp storage, audit, and
  ResultStream metadata outside the database truth boundary;
- confirming that it states the dirty-branch caveat instead of claiming release
  readiness.

The following commands are appropriate for validating future implementation or
manifest changes governed by this ADR:

```powershell
cargo metadata --no-deps --format-version 1
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture
```

For Rust source changes, add:

```powershell
cargo fmt --all --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

For C5 WAL, storage, transaction, catalog publication, recovery, backup,
restore, HA/DR, security, or RPC changes, add the owner-specific crash/recovery,
property, fuzz, Miri, threat-model, audit, and compatibility gates named by the
owning subsystem.

## Risks

- A broad crate can hide new ownership behind an existing facade unless owner
  tests and topology tests stay active.
- The dirty branch currently contains documentation that describes older crate
  counts. Reviewers can over-trust old ledgers unless they refresh metadata for
  the candidate packet.
- Provisional crates can be mistaken for accepted runtime owners before they
  have moved code, tests, dependency allowlists, and release evidence.
- C5 extraction can appear mechanical but change durable behavior if public
  reexports are tested without direct owner and crash/recovery evidence.
- Observability, benchmark, GPU, or ScenarioEvidence data can be overstated as
  truth unless documents preserve the cold snapshot plus durable WAL boundary.

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| A document says a crate owns a macro-engine because the crate name exists. | Require direct source ownership, tests, dependency allowlists, and this ADR's extraction gate before accepting the ownership claim. |
| A facade starts defining new canonical types for another owner. | Move the type to the canonical owner and keep the facade as a reexport only. |
| `cargo metadata` shows a different crate count than architecture ledgers. | Treat metadata as the local candidate snapshot and update ledgers in an owned documentation packet before release claims. |
| A durable-kernel crate imports SRPL, QUIC runtime, execution, benchmark, analytics, GPU, SQL, gRPC, or runtime JSON by default. | Reject the dependency or record a narrow ADR exception with exit criteria and C5 validation. |
| A proposed extraction moves WAL, page, manifest, backup, restore, or recovery bytes without golden vectors or crash/recovery tests. | Block the extraction until behavior locks and owner tests exist. |
| A tool or benchmark crate becomes a production dependency. | Move the shared contract downward into an owner crate or reject the edge. |
| A GPU or learned component is used to decide commit, recovery, catalog publication, authorization, or short MVCC visibility. | Reject the design under ADR-0016 and require CPU-backed authoritative behavior. |

## References

- `AGENTS.md`
- `docs/AGENTS.md`
- `Cargo.toml`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `docs/adr/ADR-0015-wal-commit-visibility.md`
- `docs/adr/ADR-0016-gpu-exclusion-critical-paths.md`
- `documentations/01_DOCTRINE_LEXIQUE_ARCHITECTURE_CATALOGUE_MODELIZATION.md`
- `documentations/architecture/WORKSPACE_RESTRUCTURE_BASELINE_2026.md`
- `documentations/architecture/dependency-edge-matrix-2026-05-08.md`
- `documentations/architecture/module-criticality-c0-c5-2026-05-08.md`
- `documentations/architecture/module-inventory-2026-05-08.md`
- `documentations/architecture/reexport-migration-ledger-2026-05-08.md`
- `documentations/architecture/engine-crate-mapping-2026-05-08.md`
- `documentations/governance/adr-backlog-2026-05-08.md`
- `documentations/implementation/target-crate-gap-ledger-2026-05-08.md`
