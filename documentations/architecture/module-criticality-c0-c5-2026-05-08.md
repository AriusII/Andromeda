# Module Criticality C0-C5 - 2026-05-08

## Purpose

This ledger assigns default C0-C5 criticality to the current crate and module surfaces so Step 0 governance can choose validation gates before moving code or accepting ownership claims.

## Scope

The ledger covers the current workspace modules observed on 2026-05-08. It maps existing crates and major module groups to the Andromeda criticality scale. It is a planning aid for validation and review, not an implementation acceptance record.

## Non-goals

- Do not use this ledger to claim release readiness.
- Do not use this ledger to lower validation for WAL, recovery, storage, transaction, catalog publication, security, RPC, backup, restore, or HA/DR paths.
- Do not treat benchmark, GPU, RAM, temp, or advisory evidence as durable truth.
- Do not mark a C5 path complete without crash/recovery and visibility evidence where applicable.

## Prerequisites

Read these sources before applying the ledger:

- `AGENTS.md`
- `docs/codex/andromeda-doctrine-for-codex.md`
- `docs/codex/mission-critical-change-policy.md`
- `docs/codex/rust-critical-quality-gates.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `documentations/architecture/module-inventory-2026-05-08.md`

## Procedure

1. Start with the default criticality in this ledger.
2. Escalate a module when it can affect Procedure contracts, catalog publication, transaction creation, visible commit, WAL durability, rollback, recovery, page or manifest truth, security admission, RPC frame validation, audit evidence for critical operations, backup, restore, or HA/DR.
3. Do not de-escalate C4 or C5 behavior because the code is currently small, test-only, or behind a facade.
4. Choose validation from the highest criticality reached by the change, not from the lowest-level file touched.
5. Record any uncertainty as an explicit risk instead of treating it as acceptance.

## Criticality Scale

| Level | Name | Design rule | Typical validation |
| --- | --- | --- | --- |
| C0 | Isolated experimental | Never enters a critical path. | Isolation proof, feature gating, and removal path. |
| C1 | Opportunistic | Disableable without consistency impact. | Unit tests and disablement proof. |
| C2 | Important | Measured, budgeted, and observable. | Unit and integration tests, metrics, and budget checks. |
| C3 | Critical | Versioned, audited, and policy-bound. | Contract tests, compatibility tests, audit checks, and policy gates. |
| C4 | Mission-critical | Tested in recovery or crash scenarios when it can affect state or admission. | Property tests, fuzz tests, threat-model review, crash/recovery matrix, and targeted integration gates. |
| C5 | Non-negotiable | Guaranteed by design and not bypassable. | C4 gates plus durable truth, visible-commit, replay, rollback, and release evidence. |

## Crate And Module Ledger

| Area | Current module surface | Default criticality | Escalation rule | Minimum validation posture |
| --- | --- | --- | --- | --- |
| `andromeda-error` | Typed engine errors and results. | C3 | Escalates to C5 when error handling determines recovery, commit, rollback, or catalog publication acceptance. | Contract and compile checks for ordinary changes; owner-specific C5 tests when used in durable gates. |
| `andromeda-digest` | Deterministic digest primitives. | C3 | Escalates to C5 when hashes bind WAL, catalog, contract, manifest, backup, audit, or recovery truth. | Golden vectors and deterministic roundtrip checks for digest-sensitive paths. |
| `andromeda-types` | Identifiers, `ContractHash`, `CatalogVersion`, scalar descriptors, transaction and invocation identifiers. | C3 | Escalates to C5 when identifiers gate visible state, catalog truth, transaction state, WAL records, or recovery. | Type invariant tests and affected owner gates. |
| `andromeda-time` | Engine timestamps and clocks. | C2 | Escalates to C4/C5 if timestamps become ordering, retention, recovery, or audit authority. | Clock invariant tests; recovery or audit tests when used as truth evidence. |
| `andromeda-hardware` | CPU, GPU policy, RAM, resource budget, and pipeline descriptors. | C2 | Escalates to C5 when enforcing GPU exclusion from commit, WAL, rollback, recovery, MVCC visibility, catalog publication, or security-critical paths. | Policy tests and no-critical-path import scans. |
| `andromeda-codec` | Explicit codec errors and little-endian helpers. | C3 | Escalates to C5 when used by WAL, page, manifest, backup, recovery, or network byte contracts. | Golden vectors, roundtrip tests, corruption rejection, and owner codec tests. |
| `andromeda-core` | Foundation compatibility facade plus principal identity. | C3 | Escalates to C4/C5 because C5 crates still import the facade and principal identity affects security gates. | Facade compatibility tests, topology tests, and security admission tests when principal paths change. |
| `andromeda-contract` | Procedure contracts, qualified names, catalog object descriptors, dependency edges. | C4 | Escalates to C5 when contract hashes, catalog binding, or Procedure compatibility gates affect publication or invocation. | Contract hash, compatibility, topology, and catalog/Procedure integration tests. |
| `andromeda-structured-object` | StructuredObject headers, layouts, descriptor hashing, row-count policy. | C3 | Escalates to C4/C5 when row-count metadata or layout descriptors become RPC, catalog, or recovery truth. | Descriptor hash, compatibility, and protocol projection tests. |
| `andromeda-security-contract` | Runtime-free admission, permission, policy, and surface vocabulary. | C4 | Escalates to C5 when used to reject requests before transaction creation or to enforce surface separation. | Security contract tests, admission denial tests, and no-runtime-dependency topology scans. |
| `andromeda-policy` | Admission and identity policy scaffold. | C3 | Escalates to C4/C5 when it affects IAM policy evaluation, pre-transaction rejection, or critical surface admission. | Policy matrix, admission denial, audit, and topology tests before runtime use. |
| `andromeda-resource` | Resource limit scaffold. | C2 | Escalates to C4/C5 when limits can reject work, throttle critical paths, or affect recovery and admission. | Limit validation, observability, fail-closed admission tests, and C5 path tests when used by durable kernels. |
| `andromeda-proto` | Typed payload contracts, generated messages, envelope validation, manifest and completion projections. | C4 | Escalates to C5 when protocol metadata, ResultStream sequencing, row-count policy, or manifest projection affects durable invocation evidence. | Protocol compatibility, generated validation, no-gRPC, and no-runtime-JSON scans. |
| `andromeda-rpc-protocol` | Runtime-free frame contracts, frame codec, stream roles, ResultStream sequencing, backpressure contracts. | C4 | Escalates to C5 when frame validation decides Procedure dispatch, terminal completion, or visible result sequencing. | Frame codec tests, sequence tests, malformed-input tests, and runtime-free topology checks. |
| `andromeda-quic` | Transport runtime, route binding, stream concurrency, reconnect, zero-RTT, mTLS identity, HA/DR streams. | C4 | Escalates to C5 when route admission, malformed frame handling, stream role separation, or HA/DR stream behavior can affect transaction creation or recovery. | Transport contract tests, security admission tests, protocol tests, and no Admin/Application surface mixing checks. |
| `andromeda-catalog` | DefinitionBatch, catalog store, publication subscription, Procedure Store, statistics, plan cache, catalog WAL records, scenario evidence. | C4 by default, C5 for publication and System Database truth. | Escalates to C5 for catalog version publication, DefinitionBatch begin/apply/commit, catalog WAL replay, Procedure contract binding, and System Database correctness. | Catalog store, DefinitionBatch, publication recovery, WAL payload, compatibility, and topology gates. |
| `andromeda-procedure-store` | Procedure Store identity, status, evidence, sink, and error scaffold. | C3 | Escalates to C4/C5 when terminal invocation evidence, retry decisions, audit correlation, or cataloged Procedure history becomes durable authority. | Procedure Store contract, retention, durability, replay, and audit-correlation tests. |
| Extracted SRPL model crates | `andromeda-srpl-ast`, `andromeda-srpl-cardinality`, `andromeda-srpl-diagnostics`, `andromeda-srpl-ir`, `andromeda-srpl-lexer`, `andromeda-srpl-parser`. | C3 | Escalates to C4/C5 when lexer, parser, binder, IR, or diagnostics affect Procedure contract acceptance, DefinitionBatch publication, or catalog binding. | Lexer/parser, AST, cardinality, diagnostics, IR tests; catalog-store-free topology gates; fuzz checks for parser inputs. |
| `andromeda-srpl` | Compiler facade, binder, lowering, interpreter, optimizer, DefinitionBatch bridge, procedure resolver. | C3 by default, C4/C5 for Procedure and catalog bridges. | Escalates to C5 when lowering or bridge logic affects catalog publication, contract hashes, or recoverable Procedure behavior. | SRPL compiler, DefinitionBatch compatibility, parser fuzz compile checks, and catalog integration tests. |
| `andromeda-wal` | LSNs, WAL records, frame codec, record bounds, durability fence helpers, FileWal byte contract and scan surface. | C5 | Always C5 when behavior changes affect persistent WAL bytes, durable LSN, scan boundaries, FileWal open/append/flush, or commit visibility. | WAL owner tests, property tests, golden vectors, fuzz targets, and storage integration tests. |
| `andromeda-storage` | Page, heap, B+Tree, buffer pool, manifest, recovery, backup, restore, HA/DR, WAL compatibility, file WAL recovery projection. | C5 | Always C5 for durable truth, page or manifest publication, WAL consumption, backup, restore, PITR, HA/DR, recovery startup, or visible storage state. | Storage owner tests, crash/recovery matrix, property tests, fuzz targets, manifest/page/heap/B+Tree/backup/restore gates. |
| `andromeda-tx` | Transaction state, MVCC, locks, savepoints, commit log, WAL adapter, GC. | C5 | Always C5 for visible commit, rollback, lock release, MVCC visibility, WAL adapter behavior, transaction replay, or version reclamation. | Transaction lifecycle, MVCC, commit log, WAL replay, lock manager, recovery, and concurrency tests. |
| `andromeda-exec` | Admission, dispatch, invocation, local runtime, result stream, SRPL adapters, audit traces, WAL evidence, vertical slice. | C4 by default, C5 for commit visibility and durable Procedure execution. | Escalates to C5 when it creates transactions, commits, rolls back, publishes terminal results, or proves recovery-visible Procedure state. | Admission, IAM, ResultStream, transaction bridge, durable vertical path, and recovery visibility tests. |
| `andromeda-observe` | Emitters, events, audit evidence, query, restore trace, principal binding, exporters. | C3 | Escalates to C4/C5 for durable audit evidence, security-critical operation trails, backup/restore evidence, or forensic startup reports. Audit still must not become database truth. | Audit family, sink, retention, corruption, trace correlation, and restore-trace tests. |
| `andromeda-maps` | Map descriptor and error scaffold. | C1/C2 | Must not escalate to C5 authority by itself. Escalates only when a future validated map refresh path is tied to cataloged analytical contracts with explicit source truth. | Descriptor tests, summarizability tests, refresh validation, and advisory-only guards. |
| `andromeda-cli` | Operator commands for audit, benchmark, catalog, protocol, recovery, backup, restore, HA/DR, and vertical checks. | C2 by default, C4 for Admin and recovery operations. | Escalates to C5 if command behavior can mutate durable state, initiate restore, promote HA/DR state, or claim release evidence. | CLI command tests, path-specific integration tests, and surface-separation checks. |
| `andromeda-bench` | Benchmark harnesses, scenario evidence, regression detection, workload records. | C1/C2 | Must not escalate to C5 authority. It can inform decisions only as advisory, expirable, bounded, explainable evidence. | Benchmark evidence tests, stale evidence rejection, and advisory-only guards. |

## C5 Claims That Remain Blocked

The current workspace shape does not by itself prove:

- WAL owner extraction is release-complete.
- Storage recovery integration is release-complete.
- B+Tree mutation durability and crash recovery are complete.
- Catalog DefinitionBatch publication is C5-complete.
- Durable Procedure execution is C5-complete.
- QUIC, IAM, and audit integration are C4/C5-complete.
- Backup, restore, PITR, HA/DR, quorum, fencing, or ForensicStart are release-ready.
- Benchmark or ScenarioEvidence output is authoritative.
- Maps, policy, resource, codec, or Procedure Store scaffolds are accepted runtime owners.

## Validation

For C4/C5 work, use the strongest applicable gate:

```powershell
cargo fmt --all --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture
```

Add subsystem gates for the highest criticality touched:

- WAL/storage/recovery: property tests, fuzz tests, golden vectors, crash/recovery matrix, and forensic startup tests.
- Transaction/MVCC: lifecycle, rollback, replay, visibility, lock, and concurrency tests.
- Catalog/Procedure contracts: compatibility, DefinitionBatch, WAL payload, publication, and replay tests.
- RPC/security: malformed frame, route admission, mTLS/principal binding, authorization denial, audit, and surface-separation tests.
- Backup/restore/HA/DR: PITR, exact LSN, quorum, fencing, promotion, and recovery report tests.

## Troubleshooting

If a change touches multiple rows, validate at the highest criticality. A C2 module used by a C5 path is validated as C5 for that change.

If a module is only a compatibility facade, classify the underlying behavior by the canonical owner. Facade compatibility tests prove import stability, not ownership or release readiness.

If a test is skipped because the worktree is dirty or a dependency is unresolved, record the skip. Do not replace a missing C5 gate with a documentation assertion.

## References

- `AGENTS.md`
- `docs/codex/andromeda-doctrine-for-codex.md`
- `docs/codex/mission-critical-change-policy.md`
- `docs/codex/rust-critical-quality-gates.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `documentations/00_ANDROMEDA_INDEX_ET_MODE_DE_LECTURE.md`
- `documentations/testing/step-11-validation-matrix.md`
- `documentations/architecture/module-inventory-2026-05-08.md`
