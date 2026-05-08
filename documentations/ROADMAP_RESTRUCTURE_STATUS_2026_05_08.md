# Roadmap Restructure Status - 2026-05-08

## Purpose

This worker status cross-check filters the external Andromeda implementation roadmap against the current local branch progress in `C:/Users/Arius/RustroverProjects/Andromeda`.

The external roadmap's initial 11-crate baseline is superseded by the current branch shape: the root `Cargo.toml` now declares a 94-crate Rust 2024 workspace. That branch shape is observable, but it is not validated. Many crates remain scaffolds, runtime-free vocabulary crates, or compatibility facades. The worktree and index are dirty across many code and documentation paths, and this document must not be read as acceptance evidence for those changes.

## Scope

This document covers only roadmap status and next write-wave planning for the workspace restructure. It does not validate runtime correctness, persistent format compatibility, network behavior, recovery semantics, security behavior, or release readiness.

Evidence used:

- `Cargo.toml` workspace members, observed on 2026-05-08.
- `git status --short --branch`, observed on 2026-05-08.
- `git diff --name-status` and `git diff --cached --name-status`, observed on 2026-05-08.
- `documentations/ROADMAP_IMPLEMENTATION_2026.md`.
- `documentations/WORKER_EXECUTION_MATRIX_2026.md`.
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`.
- `documentations/ANDROMEDA_ROADMAP_IMPLEMENTATION_CROSSCHECK_2026.md`.
- `documentations/testing/spec-validation-matrix-2026-05-08.md`.
- `documentations/governance/step-12-documentation-closure-2026-05-08.md`.

Non-goals:

- Do not claim the dirty branch passes compile, test, clippy, crash/recovery, fuzz, Miri, audit, or release gates.
- Do not override ADR-0011 or any accepted governance decision.
- Do not introduce new crate ownership doctrine.
- Do not edit, stage, unstage, revert, or commit any file outside this document.

## Current Branch Snapshot

| Item | Status | Evidence | Interpretation |
|---|---|---|---|
| Branch | Partial | `codex/workspace-crate-restructure` | The branch is dedicated to crate restructuring and integration work. |
| Workspace size | Done as branch shape, unvalidated as accepted state | Root `Cargo.toml` lists 94 workspace members. | The 11-crate baseline in the older cross-check is obsolete for this branch. |
| Dirty worktree | Blocked | `git status --short --branch` reports many `M`, `MM`, `A`, `AD`, and deleted/added pairs. | Validation is blocked until the index and worktree are reconciled by owning workers. |
| Staged changes | Blocked | `git diff --cached --name-status` reports many staged additions and modifications. | Some files have staged content that differs from the worktree. This worker must not unstage or rewrite them. |
| Unstaged changes | Blocked | `git diff --name-status` reports many modifications and deletions. | Current local files are not a clean validation target. |
| Cargo metadata | Blocked | `cargo metadata --no-deps --format-version 1` attempted rustup component sync and failed while downloading/renaming `clippy`. | Cargo-based validation cannot be trusted from this worker run. |

## Superseded Baseline

The older roadmap cross-check describes a workspace with these 11 crates:

- `andromeda-bench`
- `andromeda-cli`
- `andromeda-catalog`
- `andromeda-core`
- `andromeda-exec`
- `andromeda-observe`
- `andromeda-proto`
- `andromeda-quic`
- `andromeda-srpl`
- `andromeda-storage`
- `andromeda-tx`

The current branch declares 94 workspace crates.

Representative grouping (non-exhaustive):

| Ring or role | Current crates (examples) |
|---|---|
| Foundation and compatibility | `andromeda-core`, `andromeda-digest`, `andromeda-error`, `andromeda-hardware`, `andromeda-time`, `andromeda-types` |
| Contracts, protocol, and language model | `andromeda-contract`, `andromeda-proto`, `andromeda-rpc-protocol`, `andromeda-security-contract`, `andromeda-srpl`, `andromeda-srpl-ast`, `andromeda-srpl-cardinality`, `andromeda-srpl-diagnostics`, `andromeda-srpl-ir`, `andromeda-srpl-parser`, `andromeda-structured-object` |
| Durable kernel and execution | `andromeda-storage`, `andromeda-tx`, `andromeda-wal`, `andromeda-exec` |
| Transport runtime | `andromeda-quic` |
| Observability, tools, and evidence | `andromeda-observe`, `andromeda-cli`, `andromeda-bench` |
| Fuzz workspace member | `fuzz` |

Status: the 94-crate shape supersedes the external 11-crate baseline for planning. It does not supersede validation gates. Multiple crates remain scaffold-only, behavior-free, or facade-level. The branch still needs clean-index validation before any worker can state that the 94-crate restructure is accepted.

## Step 12 Documentation Trace

Step 12 is documentation and governance trace work. It does not validate
runtime correctness, approve release readiness, or convert dirty-branch state
into accepted implementation evidence.

### Canonical Path Mapping

Older roadmap and worker documents can still cite product documentation by
`docs/*` names. The canonical product documentation paths are now under
`documentations/*`.

| Historical roadmap path | Canonical path | Status interpretation |
|---|---|---|
| `docs/00_ANDROMEDA_INDEX_ET_MODE_DE_LECTURE.md` | `documentations/00_ANDROMEDA_INDEX_ET_MODE_DE_LECTURE.md` | Canonical protected reading-order file. |
| `docs/01_DOCTRINE_LEXIQUE_ARCHITECTURE_CATALOGUE_MODELIZATION.md` | `documentations/01_DOCTRINE_LEXIQUE_ARCHITECTURE_CATALOGUE_MODELIZATION.md` | Canonical protected doctrine and architecture file. |
| `docs/02_TYPE_SYSTEM_SRPL_PROCEDURES_MAPS.md` | `documentations/02_TYPE_SYSTEM_SRPL_PROCEDURES_MAPS.md` | Canonical protected type-system, SRPL, Procedure, and Map file. |
| `docs/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md` | `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md` | Canonical protected transaction, WAL, MVCC, storage, and recovery file. |
| `docs/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md` | `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md` | Canonical protected protocol, security, HA/DR, and operations file. |
| `docs/05_OPTIMIZER_STATS_ANALYTICS_HARDWARE_ROADMAP_SOURCES.md` | `documentations/05_OPTIMIZER_STATS_ANALYTICS_HARDWARE_ROADMAP_SOURCES.md` | Canonical protected optimizer, statistics, analytics, hardware, and roadmap source file. |
| `docs/ANDROMEDA_ROADMAP_IMPLEMENTATION_CROSSCHECK_2026.md` | `documentations/ANDROMEDA_ROADMAP_IMPLEMENTATION_CROSSCHECK_2026.md` | Historical roadmap cross-check retained as planning evidence. |
| `docs/CURRENT_STATE.md` | `documentations/CURRENT_STATE.md` | Canonical current-state summary. |
| `docs/ROADMAP_IMPLEMENTATION_2026.md` | `documentations/ROADMAP_IMPLEMENTATION_2026.md` | Canonical implementation roadmap. |
| `docs/WORKER_EXECUTION_MATRIX_2026.md` | `documentations/WORKER_EXECUTION_MATRIX_2026.md` | Canonical worker execution matrix. |
| `docs/ROADMAP_RESTRUCTURE_STATUS_2026_05_08.md` | `documentations/ROADMAP_RESTRUCTURE_STATUS_2026_05_08.md` | Canonical restructure status and dirty-branch planning record. |

`docs/adr/*` remains the Codex tooling ADR location and is intentionally cited
as governance evidence. Do not create duplicate product copies of those ADRs in
`documentations/`.

### Step 12 Closure Artifacts

| Artifact | Path | Status |
|---|---|---|
| Spec-to-test matrix | `documentations/testing/spec-validation-matrix-2026-05-08.md` | Added as mapping evidence only; it does not claim test execution. |
| Closure checklist | `documentations/governance/step-12-documentation-closure-2026-05-08.md` | Added as documentation closure evidence only; it does not approve release readiness. |
| ADR backlog | `documentations/governance/adr-backlog-2026-05-08.md` | Updated to point Step 12 ADR candidates at accepted ADR files where present. |

## Filtered Roadmap Status

| Roadmap area | Status | Current evidence | Required correction before acceptance |
|---|---|---|---|
| Workspace split from 11 to 94 crates | Partial | Root `Cargo.toml` lists the expanded workspace; ADR-0011 describes foundation, contract, SRPL, protocol, security, and WAL owner boundaries. | Reconcile staged and unstaged changes, then run topology and workspace gates. |
| R0 foundation extraction | Partial | `andromeda-digest`, `andromeda-error`, `andromeda-types`, `andromeda-time`, and `andromeda-hardware` exist; `andromeda-core` remains a compatibility facade. | Prove `andromeda-core` stays thin and that R0 crates do not gain engine, runtime, benchmark, analytics, SQL, JSON, GPU execution, or native-layout persistence dependencies. |
| Contract and StructuredObject ownership | Partial | `andromeda-contract` and `andromeda-structured-object` exist; ADR-0011 records facade reexports through catalog/proto. | Run contract owner tests, facade compatibility tests, and dependency allowlist tests from a clean worktree. |
| Security vocabulary split | Partial | `andromeda-security-contract` exists and ADR-0011 defines it as runtime-free vocabulary, not IAM runtime. | Prove no durable IAM store, revocation store, policy store, audit ledger, QUIC/TLS runtime, async runtime, SQL, JSON, GPU, or storage dependency entered the crate. |
| SRPL language-model split | Partial | `andromeda-srpl-ast`, `andromeda-srpl-cardinality`, `andromeda-srpl-diagnostics`, `andromeda-srpl-ir`, and `andromeda-srpl-parser` exist; `andromeda-srpl` remains a facade. | Validate extracted crates have no catalog-store, execution, storage, transaction, transport runtime, benchmark, analytics, GPU, or unapproved dev-dependency drift. |
| RPC protocol split | Partial | `andromeda-rpc-protocol` exists; ADR-0012 keeps it runtime-free and keeps concrete QUIC behavior in `andromeda-quic`. | Prove `andromeda-rpc-protocol` has no Quinn dependency and that `andromeda-quic` does not own Procedure semantics, storage truth, or authorization policy. |
| WAL owner split | Partial | `andromeda-wal` exists; ADR-0011 marks it as owner for pure WAL primitives and later physical FileWal byte contracts. | Run direct `andromeda-wal` owner tests plus storage integration compatibility tests. |
| Storage recovery integration | Partial | Roadmap documents require storage to keep recovery reports, replay planning, manifest/page integration, and durable visibility integration. | Add or re-run storage recovery integration evidence after WAL owner validation passes. |
| Execution durable vertical path | Partial | Roadmap says `ProductStock` durable heap integration is the near-term P0 path; dirty diffs include execution and storage paths. | Prove the default execution path uses durable heap/page/WAL state and reconstructs after crash. |
| Catalog, DefinitionBatch, Procedure Store, statistics, and PlanCache evidence | Partial | Current roadmap records completed scaffolds and remaining runtime wiring. Dirty diffs include catalog contracts, Procedure Store, statistics, plan cache, and SRPL DefinitionBatch paths. | Prove all Invocation, plan, audit, manifest, and benchmark evidence paths use validated `ProcedureContractBinding`, `CatalogVersion`, `StatsVersion`, `PolicyVersion`, and `ContractHash`. |
| QUIC, IAM, and audit integration | Partial | Roadmap and matrix describe pre-transaction `SecurityAdmission v0`, route binding, durable audit, and audit CLI vocabulary. Dirty diffs include QUIC, exec IAM, observe audit, and CLI audit paths. | Prove wrong surface, disabled principal, missing permission, and malformed frame fail before transaction creation and emit required audit evidence. |
| Benchmark and ScenarioEvidence | Partial | `andromeda-bench` exists and roadmap keeps ScenarioEvidence advisory and non-authoritative. Dirty diffs include benchmark history and scenario boundary paths. | Prove evidence is bounded, expirable, version-bound, and ignored when unsafe or stale. |
| GPU runtime | Missing by design | No dedicated `andromeda-gpu` workspace crate is declared. Roadmap says GPU is optional and later than CPU statistics and Maps. | Do not add GPU runtime until CPU statistics and Maps are stable. Any GPU crate must be optional and off commit, WAL, rollback, recovery, MVCC visibility, catalog publication, and security-critical paths. |
| Maps and analytics | Missing or scaffold-only | Roadmap places Maps after statistics and optimizer stabilization; no dedicated Map owner crate is declared. | Implement only after durable Procedure path, statistics publication, and minimal optimizer evidence are stable. |
| HA/DR, backup, PITR, and ForensicStart | Partial | Roadmap records gates and contracts; dirty diffs include storage backup/restore and CLI admin backup/restore paths. | Add full backup/restore drills, exact LSN restore tests, cluster simulation, fencing proof, and recovery reports. |
| Enterprise release readiness | Missing | Dirty branch blocks release evidence. | Run clean workspace gates, release gates, audit scans, crash/recovery tests, and documentation acceptance checks before release claims. |

## Blocked By Dirty Index

The following work should not be accepted or promoted until owning workers reconcile the index and worktree:

- Any claim that the 94-crate workspace compiles.
- Any claim that topology guards pass for the new crate rings.
- Any claim that WAL owner extraction is complete.
- Any claim that storage recovery integration is complete.
- Any claim that QUIC routing, IAM admission, or durable audit integration is complete.
- Any claim that benchmarks or ScenarioEvidence are safe to publish into catalog or optimizer paths.
- Any claim that documentation reflects the accepted repository state rather than the current branch intent.

The dirty index is especially material because `git status` reports files with both staged and unstaged content. That means a command can validate neither the staged candidate nor the final working tree unless the owning workers first decide which content is authoritative.

## Recommended Next Write Waves

### Wave 0 - Index Reconciliation And Evidence Freeze

Purpose: make the branch auditable without changing architecture.

Recommended ownership:

- Each current code owner reconciles only their own staged and unstaged files.
- Documentation owners update only status documents and ADR references.
- No worker stages, unstages, reverts, or commits another worker's files.

Exit criteria:

- `git status --short` has no mixed `MM` or `AD` entries for files under active validation.
- The branch has a single intended candidate state.
- The validation log records exact command, date, branch, and failure or pass result.

### Wave 1 - Topology And Foundation Guard

Purpose: prove that the 94-crate shape respects the dependency rings.

Recommended focus:

- R0 crates and `andromeda-core` facade.
- `andromeda-contract`, `andromeda-structured-object`, and `andromeda-security-contract`.
- SRPL extracted language-model crates.
- `andromeda-rpc-protocol` runtime-free boundary.

Exit criteria:

- Topology tests reject forbidden edges and generic crate buckets.
- R0 and contract crates pass strict allowlists.
- Extracted SRPL model crates stay catalog-store-free.
- `andromeda-rpc-protocol` stays Quinn-free.

### Wave 2 - WAL Owner And Storage Integration

Purpose: separate owner evidence from compatibility and recovery integration evidence.

Recommended focus:

- `andromeda-wal` owner tests for WAL primitives, codecs, record bounds, durability fences, scan-prefix validation, and FileWal byte contracts.
- `andromeda-storage` compatibility reexports and recovery integration tests.
- Fuzz seed generation and WAL roundtrip targets.

Exit criteria:

- Direct `andromeda-wal` tests pass.
- Storage compatibility tests prove public imports and recovery consumers still work.
- Crash/recovery tests cover any path that affects visible commit, replay, page flush, manifest publication, or recovery startup.

### Wave 3 - P0 Durable Procedure Path

Purpose: move from scattered scaffolds to one recoverable vertical path.

Recommended focus:

- `Inventory.ProductStock` execution uses durable heap/page/WAL state by default.
- Catalog DefinitionBatch publication is WAL-covered and replayable through durable catalog store paths.
- Procedure Store emits terminal records for committed, failed, and aborted outcomes.
- ResultStream metadata precedes payload and includes exact row-count policy.

Exit criteria:

- A cataloged Procedure mutates durable heap/page state.
- Visible commit occurs only after durable WAL.
- Recovery reconstructs the Procedure-visible state after crash.
- Invocation evidence ties contract, catalog, statistics, policy, plan, WAL, audit, and row-count data together.

### Wave 4 - Security, RPC, Audit, And Operations

Purpose: harden the Application Surface without mixing Admin or Cluster capabilities into it.

Recommended focus:

- QUIC route admission and typed frame validation.
- `SecurityAdmission v0` before transaction creation.
- Durable audit rejection evidence.
- Operator CLI wording: `audit inspect`, `audit verify`, and `audit compact`.
- Backup, restore, PITR, quorum, fencing, and ForensicStart drills.

Exit criteria:

- Wrong surface, disabled principal, missing permission, and malformed frame create no transaction.
- Audit evidence is emitted for rejection and terminal outcomes where required.
- Application, Admin, and Cluster surfaces remain separate.
- Restore drills prove exact target LSN behavior and recovery reports.

### Wave 5 - Optimizer, Statistics, Maps, Benchmarks, And GPU Later

Purpose: add adaptive behavior only after durable truth and recovery are proven.

Recommended focus:

- CPU statistics publication and `StatsVersion` validation.
- Minimal bounded optimizer and `PlanCacheKey`.
- ScenarioEvidence as advisory only.
- Maps and analytics after CPU paths are stable.
- Optional GPU only after CPU statistics and Maps are stable.

Exit criteria:

- Optimization decisions are bounded, versioned, observable, explainable, and disableable.
- ScenarioEvidence is never authoritative.
- GPU remains absent from commit, WAL, rollback, recovery, MVCC visibility, catalog publication, and security-critical paths.

## Validation Commands

Run these after Wave 0 produces a clean candidate. Do not treat prior pass records from 2026-05-07 as current evidence for this dirty 2026-05-08 branch.

### Documentation And Codex Tooling

```powershell
python .codex/scripts/validate_codex_tooling.py
```

### Workspace Gates

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features
cargo test --doc --workspace
```

`cargo check --workspace --all-targets --all-features` is a build-continuity
signal only. It is not release approval and it is not C5 crash/recovery
approval.

### Topology And Doctrine Gates

```powershell
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture
```

Add or run existing doctrine scanners for:

- No ad hoc SQL application surface.
- No gRPC protocol surface.
- No runtime JSON default for typed protocol payloads.
- No GPU critical-path import.
- No Rust native struct layout serialization for disk or network.

### WAL Owner And Storage Integration

```powershell
cargo test -p andromeda-wal --tests
cargo test -p andromeda-wal --test file_wal_contract -- --nocapture
cargo test -p andromeda-storage --test api_compat_reexports -- --nocapture
cargo test -p andromeda-storage --test wal_ownership_invariants -- --nocapture
cargo test -p andromeda-storage --test file_wal_recovery_contract -- --nocapture
python fuzz/generators/generate_seed_corpus.py --check
cargo check --manifest-path fuzz/Cargo.toml --bin wal_record_roundtrip --locked
```

Current observed posture for this status packet is fuzz preflight only.
Sustained fuzzing was not executed.

## Remaining Required Gates (Not Executed Here)

- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo nextest run --workspace --all-features`
- `cargo test --doc --workspace`
- `cargo audit`
- `cargo deny check`
- Sustained fuzz campaigns with retained artifacts
- Targeted Miri evidence
- Targeted Loom evidence
- Combined C5 crash/recovery matrix (WAL, storage, tx, execution, backup/restore, HA/DR)
- Release gate chain with retained evidence artifacts

### P0 Vertical Recovery Gates

Use the exact owning-crate tests once the owning workers finalize names. The minimum required coverage is:

- Procedure admission rejects invalid contract, surface, principal, permission, and resource evidence before transaction creation.
- A valid cataloged Procedure mutates durable heap/page state only after WAL coverage.
- Recovery reconstructs the committed Procedure-visible state.
- Recovery rejects incomplete catalog DefinitionBatch publication sequences.
- ResultStream metadata precedes payload and carries exact row-count policy.
- Procedure Store records committed, failed, and aborted outcomes with contract, catalog, statistics, policy, plan, WAL, row, temp, and error evidence.

## Risks And Mitigations

| Risk | Severity | Mitigation |
|---|---|---|
| Dirty index hides the actual candidate state. | High | Reconcile staged and unstaged changes by owner before validation. Do not accept broad claims while `MM` or `AD` entries remain. |
| The 94-crate shape is mistaken for validated architecture. | High | Treat workspace members as branch intent until topology, compile, tests, and doctrine gates pass. |
| `andromeda-core` regains hidden ownership through facade growth. | High | Keep facade tests active and migrate callers to precise foundation crates. |
| WAL owner evidence and storage recovery integration evidence get conflated. | High | Validate `andromeda-wal` owner tests separately from `andromeda-storage` compatibility and recovery tests. |
| Security vocabulary is overstated as IAM runtime. | Medium | Keep `andromeda-security-contract` documented and tested as runtime-free vocabulary only. |
| AuditLedger is overstated as transaction truth or immutable across compaction. | Medium | Keep audit evidence separate from commit-path truth; document retention compaction caveats. |
| ScenarioEvidence or benchmarks influence planning as truth. | Medium | Keep evidence advisory, expirable, version-bound, explainable, and ignored when unsafe. |
| GPU work arrives too early. | Medium | Defer optional GPU runtime until CPU statistics and Maps are stable and validated. |
| External roadmap task order bypasses durable truth. | High | Keep P0 focused on Procedure, Catalog, SRPL IR, Admission, Transaction, WAL, Heap/Page, Recovery, and ResultStream. |

## References

- `AGENTS.md`
- `Cargo.toml`
- `documentations/ROADMAP_IMPLEMENTATION_2026.md`
- `documentations/WORKER_EXECUTION_MATRIX_2026.md`
- `documentations/ANDROMEDA_ROADMAP_IMPLEMENTATION_CROSSCHECK_2026.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `docs/adr/ADR-0012-quic-rpc-no-grpc.md`
- `documentations/specs/FrameHeader_RPC_v0.md`
- `documentations/specs/SecurityAdmission_v0.md`
- `documentations/specs/AuditLedger_v0.md`
- `documentations/testing/spec-validation-matrix-2026-05-08.md`
- `documentations/governance/step-12-documentation-closure-2026-05-08.md`
