# Roadmap Execution Plan - 2026-05-08

## Purpose

Record the executable status of the workspace restructure roadmap against the
current local Andromeda branch.

This plan translates the external 12-step roadmap into a safe execution order
for the current 94-crate Rust workspace. It separates branch shape from
validated acceptance evidence and keeps C5 storage, WAL, recovery, transaction,
security, RPC, catalog publication, and HA/DR work out of broad refactors while
the worktree is dirty.

## Scope

This document covers:

- The roadmap prerequisite step and the 12 execution steps.
- The current 94-crate workspace shape observed in the root `Cargo.toml`.
- Acceptance gates needed before each phase can be treated as accepted.
- A phased execution order for future workers.
- Dirty-worktree constraints that prevent unsafe C5 refactors.

Owned companion detail lives in
`documentations/implementation/target-crate-gap-ledger-2026-05-08.md`.

## Non-goals

- Do not claim that the dirty branch compiles or passes tests.
- Do not approve any C5 behavior, persistent format, wire format, security
  behavior, or release readiness claim.
- Do not create, remove, rename, move, stage, unstage, or commit Rust crates.
- Do not resolve staged versus unstaged differences in files owned by another
  worker.
- Do not replace ADR-0011, ADR-0012, existing DEC records, or future accepted
  ADRs.
- Do not introduce GPU, benchmark, RAM, temp storage, or audit evidence as
  database truth.

## Prerequisites

Before using this plan for implementation, complete these checks:

1. Confirm the branch and candidate state with path-specific inspection.
2. Reconcile `MM` and `AD` entries only by the owner of those files.
3. Keep every commit or validation packet scoped to one responsibility boundary.
4. Use reexports or compatibility facades before removing historical public
   paths.
5. Require source evidence for every implementation-ready claim.
6. Require crash/recovery, property, fuzz, Miri, threat-model, or topology
   evidence before promoting C5 paths.

## Current Branch Snapshot

| Item | Current status | Evidence | Interpretation |
|---|---|---|---|
| Branch | Active restructure branch | `codex/workspace-crate-restructure` | Work is already in a restructure wave, not at the external roadmap baseline. |
| Worktree | Dirty | `git status --short` reports broad `M`, `MM`, `A`, `AD`, and untracked paths. | Treat the tree as branch intent, not acceptance evidence. |
| Workspace shape | 94 crates under `crates/` | Root `Cargo.toml` workspace members | The older 11-crate baseline is superseded for planning. |
| Current files owned by this task | Step 12 documentation trace files | Roadmap status, roadmap execution plan, documentation indexes, spec-to-test matrix, closure checklist, and ADR backlog files under `documentations/` | No code, Cargo, crate README, test source, CI, hook, or Rust source file is changed by this task. |
| Validation posture | Documentation-only consistency validation | Targeted repository reads and path-specific diff checks | No Rust build or broad gate is a valid clean-candidate proof while the tree remains dirty. |

## Current 94-Crate Reality

The current root workspace declares 94 crates. Many are not yet final behavior
owners: several remain scaffold-only, runtime-free vocabulary surfaces, or
compatibility facades.

| Ring or role | Current crates | Status interpretation |
|---|---|---|
| Foundation and compatibility | `andromeda-core`, `andromeda-digest`, `andromeda-error`, `andromeda-hardware`, `andromeda-time`, `andromeda-types` | Foundation extraction is underway. `andromeda-core` remains a temporary facade and must not regain engine ownership. |
| Contracts, protocol, and language model | `andromeda-catalog`, `andromeda-contract`, `andromeda-proto`, `andromeda-rpc-protocol`, `andromeda-security-contract`, `andromeda-srpl`, `andromeda-srpl-ast`, `andromeda-srpl-cardinality`, `andromeda-srpl-diagnostics`, `andromeda-srpl-ir`, `andromeda-srpl-parser`, `andromeda-structured-object` | Contract, StructuredObject, SRPL model, runtime-free RPC protocol, and runtime-free security vocabulary splits exist, but facade exits remain incomplete. |
| Durable kernel and execution | `andromeda-exec`, `andromeda-storage`, `andromeda-tx`, `andromeda-wal` | WAL ownership has started. Storage, transaction, recovery, and execution are still broad, high-risk owners. |
| Transport runtime | `andromeda-quic` | QUIC runtime remains separate from runtime-free frame contracts. Quinn usage must stay feature-gated and transport-owned. |
| Observability, tools, and evidence | `andromeda-observe`, `andromeda-cli`, `andromeda-bench` | Observability, CLI, and benchmark evidence exist, but benchmark and audit outputs are not database truth. |

Current-only names that do not exactly match the external target list are
intentional branch realities:

- `andromeda-rpc-protocol` is the current runtime-free owner for frame and
  ResultStream protocol contracts.
- `andromeda-security-contract` is the current runtime-free owner for security
  vocabulary and must not be treated as IAM runtime.

## Roadmap Step Register

The external roadmap includes a controlled Step 0 plus 12 execution steps.
Step 0 is a prerequisite gate for all execution work. Steps 1 through 12 are
the 12 roadmap execution steps recorded by this plan.

| Step | Roadmap focus | Current status | Do now | Blocked until |
|---:|---|---|---|---|
| 0 | Controlled freeze, inventory, baseline, and governance | Partially satisfied by current status docs, ADR backlog, and this plan | Keep branch evidence explicit and avoid broad staging or refactors | Owner reconciliation removes mixed `MM` and `AD` state from any candidate packet |
| 1 | Root layout, workspace tooling, CI, and repository hygiene | Partial | Validate root tooling and policy files in their own packet | Clean index for root/tooling paths and targeted policy-gate evidence |
| 2 | Cross-cutting foundations: types, ids, codec, digest, policy, resource, hardware | Partial | Harden existing foundation crates and identify missing `codec`, `policy`, and `resource` targets | Topology gates prove R0 crates stay engine-free |
| 3 | Contract, Catalog, DefinitionBatch, and compatibility | Partial | Keep contract and StructuredObject ownership separate from catalog store behavior | Contract owner tests, facade compatibility tests, and catalog publication crash gates |
| 4 | SRPL parser, binder, IR, lowering, diagnostics, and facade | Partial | Keep extracted SRPL model crates catalog-store-free and defer bridge splits | SRPL tests and topology gates prove no store/runtime drift |
| 5 | RPC, QUIC, Security, IAM, Audit, and Administration | Partial | Keep `andromeda-rpc-protocol` and `andromeda-security-contract` runtime-free | Protocol, route-admission, pre-transaction rejection, and durable-audit evidence |
| 6 | Transaction, WAL, MVCC, locking, and commit protocol | Partial and C5-sensitive | Freeze behavior, define durable evidence, and validate WAL owner boundaries before moving code | Clean candidate plus WAL, transaction, storage, and recovery gates |
| 7 | Storage, pages, heap, buffer pool, manifest, recovery, backup, and HA/DR | Partial and C5-sensitive | Keep storage-owned recovery integration distinct from WAL owner evidence | Golden vectors, crash/recovery drills, backup/restore drills, and manifest/page proof |
| 8 | Execution Engine, Procedure runtime, Admission, ResultStream, and Procedure Store | Partial | Preserve contract-first Invocation and metadata-before-payload behavior | Integrated Procedure path proves admission before transaction and WAL before visible commit |
| 9 | Statistics, Optimizer, Plan Cache, and DecisionTrace | Partial or scaffolded | Keep optimization bounded, versioned, observable, explainable, and disableable | `StatsVersion`, `PlanCacheKey`, DecisionTrace, and no-authoritative-evidence gates |
| 10 | Maps, analytics, benchmark workload, scenario evidence, and optional GPU | Mostly missing or intentionally deferred | Keep CPU-first analytics and ScenarioEvidence advisory | Durable Procedure path, statistics publication, and Maps stability; GPU remains later |
| 11 | Tests, fuzzing, crash runner, CI, supply chain, and release gates | Partial | Map root test labels to owner-crate suites without moving tests blindly | Recorded clean-candidate runs for workspace, C5, fuzz, Miri, Loom where applicable |
| 12 | Normative documentation, ADRs, specs, runbooks, and PR packaging | Partial | Maintain ADR backlog, specs, runbooks, packaging plan, and current-state docs | Accepted ADRs and release artifacts replace planning docs as authority |

## Step 12 Documentation Trace

Step 12 has three documentation outputs in this packet:

1. Canonical path mapping from historical roadmap `docs/*` names to current
   `documentations/*` locations.
2. A spec-to-test matrix that links v0 specifications to owner suites and
   release-gate gaps without claiming those gates passed.
3. A closure checklist that records what is complete for documentation
   navigation and what remains blocked for release acceptance.

### Canonical Path Mapping

Use this map when an older roadmap, worker matrix, or cross-check cites product
documentation under `docs/*`.

| Historical roadmap path | Canonical path | Step 12 disposition |
|---|---|---|
| `docs/00_ANDROMEDA_INDEX_ET_MODE_DE_LECTURE.md` | `documentations/00_ANDROMEDA_INDEX_ET_MODE_DE_LECTURE.md` | Canonical protected reading-order file. |
| `docs/01_DOCTRINE_LEXIQUE_ARCHITECTURE_CATALOGUE_MODELIZATION.md` | `documentations/01_DOCTRINE_LEXIQUE_ARCHITECTURE_CATALOGUE_MODELIZATION.md` | Canonical protected doctrine and architecture file. |
| `docs/02_TYPE_SYSTEM_SRPL_PROCEDURES_MAPS.md` | `documentations/02_TYPE_SYSTEM_SRPL_PROCEDURES_MAPS.md` | Canonical protected type-system, SRPL, Procedure, and Map file. |
| `docs/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md` | `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md` | Canonical protected transaction, WAL, MVCC, storage, and recovery file. |
| `docs/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md` | `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md` | Canonical protected protocol, security, HA/DR, and operations file. |
| `docs/05_OPTIMIZER_STATS_ANALYTICS_HARDWARE_ROADMAP_SOURCES.md` | `documentations/05_OPTIMIZER_STATS_ANALYTICS_HARDWARE_ROADMAP_SOURCES.md` | Canonical protected optimizer, statistics, analytics, hardware, and roadmap source file. |
| `docs/ANDROMEDA_ROADMAP_IMPLEMENTATION_CROSSCHECK_2026.md` | `documentations/ANDROMEDA_ROADMAP_IMPLEMENTATION_CROSSCHECK_2026.md` | Canonical historical roadmap cross-check. |
| `docs/CURRENT_STATE.md` | `documentations/CURRENT_STATE.md` | Canonical current-state summary. |
| `docs/ROADMAP_IMPLEMENTATION_2026.md` | `documentations/ROADMAP_IMPLEMENTATION_2026.md` | Canonical implementation roadmap. |
| `docs/WORKER_EXECUTION_MATRIX_2026.md` | `documentations/WORKER_EXECUTION_MATRIX_2026.md` | Canonical worker execution matrix. |
| `docs/ROADMAP_RESTRUCTURE_STATUS_2026_05_08.md` | `documentations/ROADMAP_RESTRUCTURE_STATUS_2026_05_08.md` | Canonical restructure status. |

Codex tooling ADRs remain under `docs/adr/`. Step 12 links to them as
governance evidence, but it does not duplicate them under `documentations/`.

### Step 12 Companion Artifacts

| Artifact | Canonical path | Use |
|---|---|---|
| Spec-to-test matrix | `documentations/testing/spec-validation-matrix-2026-05-08.md` | Maps each v0 specification to owner-suite validation and residual release gaps. |
| Closure checklist | `documentations/governance/step-12-documentation-closure-2026-05-08.md` | Records documentation closure checks without release-readiness approval. |
| ADR backlog | `documentations/governance/adr-backlog-2026-05-08.md` | Records Step 12 ADR candidates and accepted ADR references. |

## Procedure

Use the following execution order until the worktree is clean enough for
candidate validation.

### Phase 0 - Evidence Freeze And Dirty-Tree Containment

Purpose: make the branch auditable before additional code movement.

Procedure:

1. Assign every active path to an owning worker before staging or validation.
2. Resolve mixed staged and unstaged state only within the owner's write set.
3. Record branch, date, exact command, and pass or fail status for every
   validation run.
4. Keep documentation status updates separate from code packets.
5. Do not move C5 code while the target packet contains `MM` or `AD` entries.

Exit criteria:

- Candidate packets have no contradictory staged versus working-tree content.
- No packet relies on broad `git add`, `git reset`, `git clean`, or unrelated
  path changes.
- Status documents state whether they describe branch intent or accepted state.

### Phase 1 - Topology, Foundation, And Runtime-Free Contracts

Purpose: prove the 94-crate shape respects ownership rings before deeper
extraction.

Procedure:

1. Validate R0 crates and the `andromeda-core` facade.
2. Validate `andromeda-contract`, `andromeda-structured-object`, and
   `andromeda-security-contract` as runtime-free or contract-safe owners.
3. Validate `andromeda-rpc-protocol` as Quinn-free and runtime-free.
4. Keep `common`, `utils`, `misc`, `helpers`, and `god_engine` crate names
   rejected.

Exit criteria:

- Topology tests reject forbidden edges.
- Runtime-free crates have no runtime-store, Quinn, GPU, benchmark, SQL,
  runtime JSON default, or native-layout persistence dependency drift.
- Facade exceptions have documented exit criteria.

### Phase 2 - Non-C5 Language, Contract, And Protocol Splits

Purpose: reduce broad owners where the blast radius is lower than C5 runtime
behavior.

Procedure:

1. Prefer SRPL lexer, binder, lowering, diagnostics, and fixture splits before
   storage, recovery, transaction, or manifest moves.
2. Prefer protocol contract and codec ownership splits before concrete transport
   runtime changes.
3. Preserve public imports with compatibility reexports.
4. Add direct owner tests and facade compatibility tests before deleting any
   historical path.

Exit criteria:

- `andromeda-srpl-*` model crates remain catalog-store-free.
- Contract, RPC, and protocol crates keep explicit binary and wire contracts.
- No Procedure dispatch, transaction visibility, catalog publication, or storage
  recovery behavior changes by implication.

### Phase 3 - C5 Behavior Locks Before C5 Movement

Purpose: create proof targets before moving WAL, storage, recovery, transaction,
catalog publication, or security-critical code.

Procedure:

1. Define or confirm golden vectors for persistent bytes.
2. Define durable commit evidence and visible-commit rules.
3. Define storage, WAL, recovery, and catalog publication crash matrices.
4. Define security admission, wrong-surface, disabled-principal, missing
   permission, and malformed-frame rejection evidence.
5. Define audit evidence as post-fact evidence, not database truth.

Exit criteria:

- C5 move packets have behavior locks, tests, and rollback strategy before code
  movement.
- Persistent and network formats use explicit codecs and versioned rejection
  behavior.
- Recovery tests can explain incomplete, truncated, corrupt, or malformed tails.

### Phase 4 - Narrow C5 Owner Extractions

Purpose: split C5 ownership only after the candidate is coherent and testable.

Procedure:

1. Extract one boundary at a time.
2. Separate owner evidence from compatibility-facade evidence.
3. For WAL, prove direct owner tests before relying on storage integration.
4. For storage and recovery, prove page, heap, manifest, replay, and visibility
   integration together.
5. For transaction, prove prepared state remains invisible until durable WAL
   evidence allows visible commit.

Exit criteria:

- Direct owner tests pass.
- Compatibility import tests pass.
- Crash/recovery gates cover every path that can affect commit visibility,
  rollback, replay, page flush, manifest publication, catalog publication, or
  recovery startup.

### Phase 5 - Durable Procedure Path And Surface Hardening

Purpose: convert scattered evidence into one recoverable Procedure path.

Procedure:

1. Prove a cataloged Procedure mutates durable heap/page state by default.
2. Prove contract, protocol, `SecurityAdmission v0`, resource budget, and
   transaction creation order.
3. Prove ResultStream metadata precedes payload and carries exact row-count
   policy.
4. Prove rejected contract, wrong surface, disabled principal, missing
   permission, malformed frame, and unsafe early data create no transaction.
5. Prove durable audit evidence is emitted where required without becoming
   commit-path truth.

Exit criteria:

- Visible state requires durable WAL evidence.
- Recovery reconstructs Procedure-visible state after crash.
- Application, Administration, and Cluster surfaces remain separated.

### Phase 6 - Adaptive Work After Durable Truth

Purpose: add statistics, optimizer, Maps, benchmarks, and optional GPU only
after durable truth and recovery are proven.

Procedure:

1. Publish `StatsVersion` through validation and trace.
2. Keep optimizer choices bounded, versioned, observable, explainable, and
   disableable.
3. Keep ScenarioEvidence advisory, expirable, and ignored when unsafe.
4. Build Maps and analytics CPU-first after durable Procedure and statistics
   gates are stable.
5. Add optional GPU only after CPU statistics and Maps are stable, with CPU
   fallback and import scans.

Exit criteria:

- Adaptive evidence cannot override contract, WAL, recovery, catalog, or
  security truth.
- GPU remains absent from commit, WAL, rollback, recovery, MVCC visibility,
  catalog publication, and security-critical paths.

## Acceptance Gates

| Gate | Applies to | Minimum required evidence |
|---|---|---|
| Dirty-tree containment | Every packet | No broad staging, no unrelated revert, no unresolved `MM` or `AD` files in the packet, path-specific diff review. |
| Documentation consistency | Documentation-only updates | Source-grounded status, Microsoft Learn-style sections, no runtime or release overclaim. |
| Workspace topology | Foundation, contract, SRPL, protocol, durable-kernel boundaries | `workspace_dependency_topology`, `orphan_source_invariants`, and forbidden-edge scans. |
| Rust baseline | Any Rust code packet | `cargo fmt --all --check`, `cargo check`, clippy, and owner tests appropriate to the packet. |
| Contract and SRPL | Contract, StructuredObject, DefinitionBatch, SRPL model or bridge work | ContractHash golden evidence, facade compatibility, SRPL parser/model tests, DefinitionBatch dry-run and crash gates where publication is affected. |
| Protocol and security | RPC, QUIC, admission, IAM, audit | Runtime-free protocol checks, no gRPC, no runtime JSON default, route-admission tests, pre-transaction rejection tests, durable audit evidence. |
| C5 durable kernel | WAL, transaction, storage, recovery, catalog publication, backup, restore, HA/DR | Golden vectors, property tests, fuzz, crash/recovery matrix, visible-commit proof, page/manifest/replay proof, backup/restore drills where relevant. |
| Adaptive and GPU | Statistics, optimizer, Maps, analytics, benchmark, ScenarioEvidence, GPU | Bounded and versioned optimization evidence, DecisionTrace, ScenarioEvidence non-authoritative tests, GPU optionality, CPU fallback, no-C5-import scans. |
| Release readiness | Release or major merge | Clean candidate state, exact command log, commit SHA, toolchain, pass/fail status, skipped tests, unresolved gaps, ADR/DEC acceptance links. |

## Validation

This documentation-only task was validated by targeted repository inspection.
The following source classes were checked:

- Root `AGENTS.md` for invariants, Rust posture, validation guidance, and chat
  language.
- Root `Cargo.toml` and `crates/` directory listing for the current 94-crate
  workspace reality.
- `documentations/ROADMAP_RESTRUCTURE_STATUS_2026_05_08.md` for dirty-branch
  status and safe write-wave ordering.
- `documentations/architecture/WORKSPACE_RESTRUCTURE_BASELINE_2026.md` for
  target-crate gaps and dirty-worktree constraints.
- `docs/adr/ADR-0011-workspace-crate-boundaries.md` for dependency rings,
  facade exceptions, and durable-kernel extraction rules.
- The external roadmap file referenced by the repository baseline for the
  12-step execution roadmap and target crate names.
- `documentations/testing/step-11-validation-matrix.md` and
  `documentations/testing/spec-validation-matrix-2026-05-08.md` for Step 11 and
  spec-to-test mapping.
- `documentations/governance/adr-backlog-2026-05-08.md` and
  `documentations/governance/step-12-documentation-closure-2026-05-08.md` for
  Step 12 documentation closure evidence.

No Rust build, Cargo test, clippy, audit, deny, or Codex tooling validation was
run because this task changes only documentation trace, index, governance, and
testing-matrix files. It does not edit Rust code, crate README files, Cargo
manifests, CI, hooks, skills, agents, or executable tests.

`cargo check --workspace --all-targets --all-features` is tracked as build
continuity only when observed from this wave context. It is not release
approval and not C5 crash/recovery approval.

Fuzz posture in this planning pass is preflight-only; no sustained fuzz
campaign evidence is claimed.

Remaining required gates:

- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo nextest run --workspace --all-features`
- `cargo test --doc --workspace`
- `cargo audit`
- `cargo deny check`
- Sustained fuzz campaigns with retained artifacts
- Targeted Miri evidence
- Targeted Loom evidence
- Combined C5 crash/recovery matrix evidence
- Release gate chain evidence package

## Troubleshooting

| Symptom | Corrective action |
|---|---|
| A future worker treats the 94-crate shape as accepted architecture. | Require topology, compile, owner tests, and doctrine gates from a clean candidate before acceptance. |
| A future packet wants to move storage, WAL, transaction, recovery, catalog publication, or security-critical code while the packet has `MM` or `AD` entries. | Stop the move. Reconcile the packet first and add behavior locks before C5 extraction. |
| A future crate split removes a historical import path in the same packet as the move. | Preserve a compatibility reexport first unless a targeted compatibility test proves removal is safe. |
| A future document says GPU, benchmark, audit, RAM, or temp output is truth. | Replace the claim with the durable truth rule: accepted cold snapshot plus durable WAL, with typed recovery evidence. |
| A future document says `andromeda-security-contract` implements IAM runtime. | Correct it to runtime-free security vocabulary and point IAM runtime work to later security/admission/audit crates. |
| A future document says an Admin RPC audit query is implemented. | Require code and tests for that endpoint; otherwise use current CLI audit wording such as `audit inspect`, `audit verify`, and `audit compact`. |

## References

- `AGENTS.md`
- `Cargo.toml`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `docs/adr/ADR-0012-quic-rpc-no-grpc.md`
- `documentations/CURRENT_STATE.md`
- `documentations/ROADMAP_IMPLEMENTATION_2026.md`
- `documentations/WORKER_EXECUTION_MATRIX_2026.md`
- `documentations/ROADMAP_RESTRUCTURE_STATUS_2026_05_08.md`
- `documentations/architecture/WORKSPACE_RESTRUCTURE_BASELINE_2026.md`
- `documentations/implementation/target-crate-gap-ledger-2026-05-08.md`
- `documentations/testing/step-11-validation-matrix.md`
- `documentations/testing/spec-validation-matrix-2026-05-08.md`
- `documentations/governance/adr-backlog-2026-05-08.md`
- `documentations/governance/step-12-documentation-closure-2026-05-08.md`
- `C:/Users/Arius/Desktop/andromeda_roadmap_restructuration_workspace_crates_engines_2026.md`
