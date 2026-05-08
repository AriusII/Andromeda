# Worker Wave Results - 2026-05-08

## Purpose

Record the concrete output left by the 2026-05-08 worker wave in the local
Andromeda worktree.

This artifact summarizes observed completed worker output by roadmap step. It
records new crates, new specifications, tests, tooling, documentation, and
known validation blockers. It does not claim that any roadmap step is accepted,
release-ready, or production-ready.

## Scope

This document covers the local workspace observed on 2026-05-08 on branch
`codex/workspace-crate-restructure` at commit
`5f053fd7efcb8a81577dfc3c197d761487837609`.

The current root `Cargo.toml` declares 32 workspace members:

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

The 32-crate shape is observed worktree output before any additional
target-crate scaffold packet. It is not acceptance evidence.

## Non-goals

- Do not claim release readiness.
- Do not claim that the dirty worktree compiles.
- Do not claim that C4 or C5 behavior is complete.
- Do not claim that new scaffold crates are final canonical owners.
- Do not treat specifications, runbooks, fuzz corpora, benchmark output,
  audit output, RAM state, temp state, or GPU policy as database truth.
- Do not treat planning indexes under `tests/` as canonical fuzz ownership;
  canonical harnesses, target registry, corpus manifest, and generator remain
  under `fuzz/`.
- Do not normalize `docs/` and `documentations/` path mapping in this packet.
  That mapping is being clarified by a separate documentation owner.
- Do not resolve, stage, unstage, revert, commit, or package another worker's
  files from this artifact.

## Prerequisites

Before using this summary as planning input, confirm:

1. The candidate packet has an explicit path owner.
2. `MM` and `AD` paths in the candidate packet have been reconciled by the
   owning worker.
3. The relevant owner tests and topology gates have been run on the exact
   candidate state.
4. C5 claims have crash/recovery, property, fuzz, Miri, Loom, security, audit,
   or protocol evidence as applicable.
5. Release claims have retained evidence with branch, commit, toolchain, exact
   command, pass or fail result, artifact path, and residual risk.

## Procedure

This document was built from source-grounded inspection:

1. Read the root `AGENTS.md` for project invariants, Rust posture, validation
   expectations, and documentation style.
2. Inspected `git status --short --branch` to identify actual modified,
   added, deleted, mixed, and untracked wave output.
3. Inspected the root `Cargo.toml` and current crate directories to identify
   the 32-crate workspace shape and new local crates.
4. Inspected implementation, architecture, governance, testing, specification,
   and operations ledgers created or modified by the worker wave.
5. Inspected representative new crate entry points for scope statements,
   dependencies, exported types, and tests.
6. Recorded blockers separately from completed output to avoid release or
   runtime overclaim.

## Roadmap Status Consolidation

The read-only group findings update how this wave output should be interpreted.

| Finding | Resulting status |
| --- | --- |
| The workspace currently has 32 root members under `crates/` before any additional target-crate scaffolds. | The new crate count is branch output only. Future scaffolds still need owner statements, topology gates, and path-local validation. |
| Many roadmap phases remain partial. | The step table records concrete output, not phase acceptance. Specs, tests, and crates are evidence targets until clean candidate runs exist. |
| `andromeda-maps` and `andromeda-procedure-store` are provisional. | Do not treat either crate as accepted runtime ownership for Maps, analytics, durable Procedure Store records, catalog publication, or execution integration. |
| Fuzz remains canonical under `fuzz/`. | Use `fuzz/targets.toml`, `fuzz/corpus/manifest.toml`, `fuzz/generators/generate_seed_corpus.py`, and `fuzz/VALIDATION_MATRIX.md` as the harness and corpus authority. `tests/fuzzing/` is an index. |
| Documentation path mapping is being clarified elsewhere. | This packet does not rewrite references between `docs/` and `documentations/`; it records only the owned roadmap-status consolidation. |
| Release blockers remain. | Dirty worktree state, C5 crash/recovery gaps, sustained fuzz gaps, Miri/Loom gaps, supply-chain/MSRV risk, and missing retained release artifacts continue to block release claims. |

## Completed Output By Roadmap Step

The table records completed worker-wave output observed in the worktree. The
status column describes validation posture, not roadmap acceptance.

| Step | Roadmap focus | Concrete completed worker-wave output | Validation posture |
| ---: | --- | --- | --- |
| 0 | Controlled freeze, inventory, baseline, and governance | Added current-state governance artifacts, including `documentations/ROADMAP_RESTRUCTURE_STATUS_2026_05_08.md`, `documentations/implementation/index.md`, `documentations/implementation/roadmap-execution-plan-2026-05-08.md`, `documentations/implementation/target-crate-gap-ledger-2026-05-08.md`, `documentations/implementation/worktree-packaging-plan-2026-05-08.md`, `documentations/architecture/dependency-edge-matrix-2026-05-08.md`, `documentations/architecture/engine-crate-mapping-2026-05-08.md`, `documentations/architecture/module-inventory-2026-05-08.md`, `documentations/architecture/module-criticality-c0-c5-2026-05-08.md`, and `documentations/architecture/reexport-migration-ledger-2026-05-08.md`. | Working audit only. The worktree remains dirty and includes mixed staged and unstaged paths. |
| 1 | Root layout, workspace tooling, CI, and repository hygiene | Added or modified root and CI tooling surfaces: `.cargo/`, `.config/nextest.toml`, `rust-toolchain.toml`, `rustfmt.toml`, `.github/scripts/roadmap_gate_summary.py`, policy and protocol scanner updates, GitHub workflow updates, `tools/testing/*`, root test-area READMEs, and benchmark documentation under `benches/`. | Tooling output exists, but root gates were not rerun by this documentation pass. Current linker and dirty-source blockers prevent release evidence. |
| 2 | Cross-cutting foundations: types, ids, codec, digest, policy, resource, hardware | Added provisional foundation crates `andromeda-codec`, `andromeda-policy`, and `andromeda-resource`. `andromeda-codec` provides dependency-free explicit little-endian helpers. `andromeda-policy` provides runtime-free policy identity and admission stance primitives. `andromeda-resource` provides typed resource budget and limit primitives. Foundation READMEs and tests also appear for existing extracted crates. | Provisional crate output only. Topology and owner tests are still required before treating these crates as accepted ownership boundaries. |
| 3 | Contract, Catalog, DefinitionBatch, and compatibility | Added or modified contract and catalog evidence surfaces: `andromeda-contract` contract hash and compatibility tests, `andromeda-catalog` contract, DefinitionBatch, procedure-store, publication, statistics, plan-cache, catalog-diff, and WAL-record paths, plus v0 specs for `CatalogObjectModel`, `CatalogDiff`, `ContractCompatibility`, `ContractHash_Canonicalization`, `DefinitionBatch`, `ProcedureContract`, `StructuredObjectPayload`, and `TypeSystem`. | Partial. Catalog contains many `MM` and `AD` paths, and catalog publication, DefinitionBatch crash gates, and compatibility evidence remain blockers before C5 acceptance. |
| 4 | SRPL parser, binder, IR, lowering, diagnostics, and facade | Added the extracted `andromeda-srpl-lexer` crate for tokenization only. Added direct owner tests for `andromeda-srpl-ast` and `andromeda-srpl-parser`. Modified `andromeda-srpl`, `andromeda-srpl-parser`, `andromeda-srpl-ir`, cardinality, diagnostics, DefinitionBatch bridge, optimizer, and facade compatibility paths. Added SRPL fuzz target surfaces for parser owner and signature decoding. | Partial. SRPL split files include mixed and `AD` paths. Parser/model crates still need topology and SRPL owner gates before acceptance. |
| 5 | RPC, QUIC, Security, IAM, Audit, and Administration | Added or modified RPC and security boundary evidence: `andromeda-rpc-protocol` frame wire and surface drift tests, QUIC route admission and surface-separation tests, security-contract admission and permission tests, execution IAM and audit-completion tests, observe durable-audit and audit-family tests, CLI audit and Admin command tests, and specs for `SecurityAdmissionCanonicalOrder`, `SurfaceSeparation`, and related protocol/security surfaces. Operator runbooks now cover audit, backup, restore, WAL pressure, failover, replica lag, slow clients, corruption suspicion, and forensic startup. | Partial. Wrong-surface, disabled-principal, missing-permission, malformed-frame, durable-audit, and no-transaction-created gates still require clean candidate evidence. No Admin RPC audit query is claimed. |
| 6 | Transaction, WAL, MVCC, locking, and commit protocol | Added or modified WAL and transaction surfaces: `andromeda-wal` codec, scan, segment, durability fence, API compatibility, property roundtrip, codec contract, and recovery corruption tests; `andromeda-tx` commit-log, replay reconstruction, WAL adapter, locking savepoint, and MVCC isolation anomaly tests; specs for `DurableCommitEvidence`, `DurableRollbackEvidence`, `MvccIsolation`, and `WalRecord`; and WAL-related fuzz targets. | C5-sensitive and partial. Durable visible-commit, rollback, WAL owner, transaction replay, and crash/recovery gates remain required. |
| 7 | Storage, pages, heap, buffer pool, manifest, recovery, backup, and HA/DR | Added or modified storage outputs for segment index, page lifecycle, buffer-pool policy, backup execution retention, HADR quorum, restore/PITR, WAL shipping, recovery, manifest/page/heap/B-Tree byte surfaces, and specs for `BackupManifest`, `BufferPoolPolicy`, `DatabaseManifest`, `HadrQuorumFencing`, `PageHeader_PageTrailer`, `PageLifecycle`, `RecoveryReport`, `RecoveryTrace`, `SegmentIndex`, and `WalShipping`. Added storage-related fuzz corpora and targets for page, heap, B-Tree, manifest, segment index, and WAL records. | C5-sensitive and partial. Storage contains many modified and `AD` paths. Golden vectors, property/fuzz evidence, WAL-before-page-flush proof, backup/restore drills, manifest recovery, and combined crash/recovery gates remain blockers. |
| 8 | Execution Engine, Procedure runtime, Admission, ResultStream, and Procedure Store | Added provisional `andromeda-procedure-store` with runtime-free invocation identity, status, evidence digest, evidence marker, and sink contracts. Modified execution admission, local runtime, remote invocation, ResultStream metadata, permission audit, and surface-gate tests. Added `ProcedureInvocationTrace` specification coverage. | Partial. A durable default Procedure path that mutates heap/page/WAL state and reconstructs after crash is not proven by this wave summary. |
| 9 | Statistics, Optimizer, Plan Cache, and DecisionTrace | Added specifications for `DecisionTrace`, `PlanCacheKey`, and `StatsObject`. Modified catalog statistics, plan-cache identity, advisory evidence, scenario evidence, and procedure feedback paths. Modified SRPL optimizer tests and benchmark scenario-boundary evidence tests. | Partial or scaffolded. Optimizer and statistics decisions still require bounded, versioned, observable, explainable, disableable evidence and clean candidate tests. |
| 10 | Maps, analytics, benchmark workload, scenario evidence, and optional GPU | Added provisional `andromeda-maps` with runtime-free Map descriptor, grain, refresh mode, and staleness primitives. Added specs for `MapDescriptor`, `MapRefreshValidation`, and `GpuBatchPolicy`. Modified benchmark history, scenario-boundary validity, regression detection, benchmark README material, and fuzz registry entries. | Planned or advisory. Maps and GPU are not accepted runtime behavior. ScenarioEvidence and benchmark output remain non-authoritative. No GPU runtime crate is present. |
| 11 | Tests, fuzzing, crash runner, CI, supply chain, and release gates | Added root testing documentation under `tests/`, fuzz documentation and target registry updates under canonical `fuzz/`, deterministic fuzz corpora, `fuzz/ROADMAP_FUZZ_GAPS_2026.md`, testing tools under `tools/testing/`, and release evidence documents including `documentations/testing/step-11-validation-matrix.md`, `ci-release-gate-evidence.md`, `fuzz-miri-loom-evidence.md`, `unsafe-miri-inventory-2026-05-08.md`, and `release-evidence-template.md`. | Evidence framework exists. Sustained fuzz, Miri, Loom, combined crash/recovery, and clean workspace gate artifacts remain missing for release claims. |
| 12 | Normative documentation, ADRs, specs, runbooks, and PR packaging | Added or updated the specification index, governance documents, ADR backlog, C4/C5 control matrix, C5 refactor freeze checklist, MSRV and dependency risk notes, release-readiness gates, risk register, supply-chain policy, operations runbooks, implementation ledgers, architecture ledgers, and ADR files for unsafe policy, binary format endian policy, WAL commit visibility, GPU exclusion, Rust toolchain/MSRV, and engine crate mapping. | Documentation output is concrete, but it does not approve runtime behavior. Current release disposition remains blocked in governance documents. |

## New Crates

The following crates are new untracked directories in the local worktree and
are declared in the root workspace. They are concrete wave output, not final
ownership approval.

| Crate | Current observed role | Current validation boundary |
| --- | --- | --- |
| `andromeda-codec` | Dependency-free explicit little-endian byte helper crate with checked read and write helpers. | Keep runtime-free and explicit-format only; add owner tests before using it for persistent or network bytes. |
| `andromeda-maps` | Runtime-free Map descriptor, grain, refresh-mode, and staleness primitive scaffold. | Do not treat as Map engine implementation until refresh, summarizability, source-data, publication, and validation gates exist. |
| `andromeda-policy` | Runtime-free policy identity and admission stance primitive scaffold. | Do not treat as IAM runtime or durable policy store. |
| `andromeda-procedure-store` | Runtime-free invocation identity, status, evidence digest, evidence marker, and sink contract scaffold. | Do not treat as durable Procedure Store runtime until catalog and execution integration gates pass. |
| `andromeda-resource` | Typed resource budget and limit primitive scaffold. | Require admission and observability integration before C4/C5 use. |
| `andromeda-srpl-lexer` | Extracted SRPL lexical scanner and token model, dependent only on SRPL diagnostics. | Keep catalog-store-free; validate through SRPL lexer/parser owner tests and topology gates. |

## New Specifications

The wave added a large v0 specification set under `documentations/specs/`.
These documents define contracts, gaps, or partial implementation evidence.
They do not prove that runtime paths are complete.

| Area | New specification artifacts observed |
| --- | --- |
| Contract, catalog, and type system | `CatalogObjectModel_v0.md`, `CatalogDiff_v0.md`, `ContractCompatibility_v0.md`, `ContractHash_Canonicalization_v0.md`, `DefinitionBatch_v0.md`, `ProcedureContract_v0.md`, `StructuredObjectPayload_v0.md`, `TypeSystem_v0.md` |
| Transaction, WAL, storage, and recovery | `BackupManifest_v0.md`, `BufferPoolPolicy_v0.md`, `DatabaseManifest_v0.md`, `DurableCommitEvidence_v0.md`, `DurableRollbackEvidence_v0.md`, `MvccIsolation_v0.md`, `PageHeader_PageTrailer_v0.md`, `PageLifecycle_v0.md`, `RecoveryReport_v0.md`, `RecoveryTrace_v0.md`, `SegmentIndex_v0.md`, `WalRecord_v0.md`, `WalShipping_v0.md` |
| Security, RPC, and surfaces | `SecurityAdmissionCanonicalOrder_v0.md`, `SurfaceSeparation_v0.md` |
| Statistics, optimization, maps, and GPU policy | `DecisionTrace_v0.md`, `GpuBatchPolicy_v0.md`, `MapDescriptor_v0.md`, `MapRefreshValidation_v0.md`, `PlanCacheKey_v0.md`, `StatsObject_v0.md` |
| Index | `index.md` |

Existing tracked specifications such as `AuditLedger_v0.md`,
`FrameHeader_RPC_v0.md`, and `SecurityAdmission_v0.md` remain part of the
current documentation surface and are referenced by the new index and
governance files, but they are not counted here as new untracked files.

## Tests And Fuzzing Output

The wave added or modified tests across owner crates and added root-level test
planning artifacts.

Canonical fuzz ownership remains under `fuzz/`. Planning documents under
`tests/fuzzing/` may index or explain fuzz evidence, but they do not replace
`fuzz/targets.toml`, `fuzz/corpus/manifest.toml`,
`fuzz/generators/generate_seed_corpus.py`, or `fuzz/VALIDATION_MATRIX.md`.

| Test area | Concrete output observed |
| --- | --- |
| Workspace topology and doctrine | Modified CLI topology and orphan-source invariant tests; added `crates/andromeda-cli/tests/workspace_dependency_topology/` and policy scanner tests under `.github/scripts/tests/`. |
| Contract and catalog | Added or modified ContractHash golden tests, procedure-contract compatibility tests, catalog diff tests, DefinitionBatch compatibility tests, catalog publication, Procedure Store, statistics, plan cache, and WAL record design tests. |
| SRPL | Added owner-direct tests for `andromeda-srpl-ast` and `andromeda-srpl-parser`; modified compiler pipeline, DefinitionBatch compatibility, optimizer diagnostics, optimizer folding, optimizer pipeline, and optimizer safety tests. |
| RPC, QUIC, and security | Added or modified RPC frame wire and forbidden-surface tests, QUIC route-admission and surface-separation tests, security-contract tests, IAM hardening, permission scope, remote invocation, and audit completion tests. |
| Storage, WAL, and transaction | Added or modified WAL roundtrip and recovery corruption tests, buffer-pool policy tests, page lifecycle tests, segment index tests, backup execution retention tests, HADR quorum tests, locking savepoint tests, and MVCC isolation anomaly tests. |
| Observability and audit | Modified audit family, durable-audit query, durable-audit retention, durable-audit sink, IO pipeline, protocol correlation, and procedure lifecycle tests. |
| Fuzzing | Added fuzz targets and corpora for contract hash, DefinitionBatch, manifest decode, segment index decode, StructuredObject payload, WAL record roundtrip, B-Tree node decode, heap page decode, page codec decode, durable audit journal decode, RPC protocol frame codec, QUIC typed frame envelope, QUIC zero-RTT admission, ResultStream sequence, SRPL parser, and security admission matrix. |
| Root validation planning | Added `tests/README.md`, `tests/crash-recovery/README.md`, `tests/fuzzing/README.md`, `tests/loom/README.md`, and `tests/miri/README.md`. |

These tests and fuzz assets are useful owner evidence targets. They are not
release evidence until run on a clean candidate with retained command output.

## Tooling Output

The wave added or modified local and CI tooling used to package, validate, and
govern future packets.

| Tooling area | Concrete output observed |
| --- | --- |
| Rust toolchain and formatting | `rust-toolchain.toml`, `rustfmt.toml`, `.cargo/config.toml`, and `.config/nextest.toml`. |
| GitHub workflows | Existing workflow updates plus a broad workflow set under `.github/workflows/`, including CI, Rust matrix, Protobuf contracts, security, supply chain, nightly validation, fuzzing, performance, cross-build, release, docs links, hygiene, governance, housekeeping, crash-recovery placeholder, and protocol doctrine scanning. |
| Policy and doctrine scripts | Modified policy gate and Protobuf manifest scripts, protocol doctrine scan tests, and added `roadmap_gate_summary.py`. |
| Testing tools | Added `tools/testing/crash_matrix_check.py`, `msrv_dependency_check.py`, `preflight.py`, `roadmap_gate_summary.py`, `step11_inventory.py`, `unsafe_inventory.py`, and validation manifest helpers. |
| Benchmark and evidence documentation | Added benchmark docs under `benches/` and scenario-evidence notes. |

Tooling output is concrete but still subject to packaging, path ownership, and
validation gates.

## Known Validation Blockers

The following blockers are active as of the inspected 2026-05-08 worktree.

| Blocker | Evidence observed | Effect |
| --- | --- | --- |
| Release blockers remain | Governance, testing, and release evidence documents still require clean candidate runs, retained artifacts, C5 crash/recovery evidence, sustained fuzz evidence, and MSRV/supply-chain validation. | No release readiness, production readiness, or phase acceptance claim can be made from this wave summary. |
| Dirty worktree | `git status --short --branch` reports broad modified, mixed, added, deleted, and untracked paths. | No release, broad compile, or acceptance claim can be made from this source state. |
| Mixed staged and unstaged files | Many paths report `MM`; many paths report `AD`. | Packet owners must reconcile path-specific staged and working-tree content before validation or packaging. |
| Current source state is not a clean candidate | Documentation ledgers and status output both classify the worktree as dirty. | Prior pass records cannot be reused as current evidence for the 2026-05-08 dirty branch. |
| Plain-shell Windows linker preflight blocked | `where.exe link` did not find `link.exe` from plain PowerShell; the Visual Studio developer command environment exposes the linker used by local Rust gates. `rustc 1.95.0` and `cargo 1.95.0` are the pinned tools. | Local Windows Rust build or test gates must run from the developer environment or from retained CI evidence. |
| MSRV and dependency evidence risk | Workspace baseline is Rust 1.95.0, while Cargo manifests and `Cargo.lock` are dirty. | Locked dependency gates under Rust 1.95.0 are required before release or package acceptance, and retained evidence remains mandatory. |
| C5 crash/recovery evidence missing | Release gate documents require combined WAL, storage, transaction, execution, backup/restore, and HA/DR crash/recovery gates. | C5 durable truth, visible commit, recovery, backup, restore, and HA/DR claims remain blocked. |
| Sustained fuzz evidence missing | Fuzz targets and seed corpora exist, but release documents classify smoke or compile checks as insufficient. | Byte, parser, protocol, ResultStream, and security-admission surfaces need sustained fuzz records before promotion. |
| Miri and Loom evidence missing where applicable | Root Miri and Loom planning docs exist; no blocking release evidence is recorded by this pass. | Unsafe, memory-sensitive, or concurrency-sensitive claims require targeted evidence or explicit residual risk. |
| Future-dated governance records are not current proof | Release risk documents flag DEC-035, DEC-036, and DEC-037 as future-dated relative to 2026-05-08. | They cannot be used as current release proof for this date. |
| Scaffold crates can be overclaimed | New crates exist but are small, runtime-free scaffolds with limited or no standalone tests. | Treat them as branch output until topology, owner tests, and facade migration gates pass. |
| Documentation path mapping is owned elsewhere | The repository currently contains both `docs/` and `documentations/` references, and a separate documentation owner is clarifying path mapping. | Do not use this wave summary to normalize paths or edit Step 12-owned documentation. |

## Validation

This documentation-only artifact was validated with targeted repository
inspection and path-local checks. The following commands were run while
preparing the artifact:

```powershell
Get-Content -Raw AGENTS.md
rg --files -g AGENTS.md
git status --short
git status --short --branch
git rev-parse HEAD
Get-Content -Raw Cargo.toml
Get-ChildItem crates -Directory | Select-Object -ExpandProperty Name | Sort-Object
rg --files documentations
rg --files documentations/specs documentations/testing documentations/governance documentations/operations/runbooks documentations/architecture
rg --files tests fuzz tools .github/scripts .github/workflows .cargo .config benches
rg -n "fuzz/|targets.toml|VALIDATION_MATRIX|corpus/manifest" fuzz tests documentations -g "*.md" -g "*.toml"
rustc -V
cargo -V
where.exe link
git diff --check -- documentations/implementation/worker-wave-results-2026-05-08.md
git status --short -- documentations/implementation/worker-wave-results-2026-05-08.md
```

No Rust build, Cargo test, clippy, nextest, audit, deny, fuzz, Miri, Loom, or
crash/recovery gate was run for this documentation-only update. Those gates
remain required for code, manifest, C4, C5, security, protocol, and release
claims.

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| A reader treats this document as release approval. | Use the Non-goals and Known Validation Blockers sections. This is a worktree output summary only. |
| A reader treats a new crate as an accepted canonical owner. | Require owner tests, topology gates, dependency allowlists, and facade exit evidence before acceptance. |
| A worker wants to package all dirty paths together. | Use `documentations/implementation/worktree-packaging-plan-2026-05-08.md` and pathspec-only packet ownership. |
| A C5 claim cites only unit tests, specs, fuzz corpora, or planning docs. | Require combined crash/recovery, property/fuzz, Miri/Loom where applicable, and retained evidence on a clean candidate. |
| A security or RPC claim cites vocabulary crates as runtime proof. | Separate runtime-free contracts from concrete IAM, QUIC, audit, and execution behavior. |
| A benchmark, ScenarioEvidence, audit, GPU, RAM, or temp result is described as truth. | Replace the claim with the durable truth rule: accepted cold snapshot plus durable WAL, with recovery evidence. |

## References

- `AGENTS.md`
- `Cargo.toml`
- `.github/scripts/`
- `.github/workflows/`
- `crates/README.md`
- `crates/andromeda-codec/`
- `crates/andromeda-maps/`
- `crates/andromeda-policy/`
- `crates/andromeda-procedure-store/`
- `crates/andromeda-resource/`
- `crates/andromeda-srpl-lexer/`
- `documentations/ROADMAP_IMPLEMENTATION_2026.md`
- `documentations/WORKER_EXECUTION_MATRIX_2026.md`
- `documentations/ROADMAP_RESTRUCTURE_STATUS_2026_05_08.md`
- `documentations/architecture/dependency-edge-matrix-2026-05-08.md`
- `documentations/architecture/engine-crate-mapping-2026-05-08.md`
- `documentations/architecture/module-inventory-2026-05-08.md`
- `documentations/governance/release-readiness-gates-2026-05-08.md`
- `documentations/governance/risk-register-2026-05-08.md`
- `documentations/implementation/roadmap-execution-plan-2026-05-08.md`
- `documentations/implementation/target-crate-gap-ledger-2026-05-08.md`
- `documentations/implementation/worktree-packaging-plan-2026-05-08.md`
- `documentations/specs/index.md`
- `documentations/testing/ci-release-gate-evidence.md`
- `documentations/testing/fuzz-miri-loom-evidence.md`
- `documentations/testing/step-11-validation-matrix.md`
- `fuzz/README.md`
- `fuzz/VALIDATION_MATRIX.md`
- `fuzz/corpus/manifest.toml`
- `fuzz/targets.toml`
- `tools/testing/`
