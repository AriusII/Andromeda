# Copilot instructions for Andromeda

Andromeda is a Rust workspace for a recoverable vertical slice of a mission-critical transactional relational database
system. Keep generated code and documentation aligned with the project doctrine in
`Andromeda_SGBDRT_SRPL_Master_Consolidation_2026.pdf`, `README.md`, `.claude/CLAUDE.md`, and `instructions/`.

## Session initialization

Treat `Andromeda_SGBDRT_SRPL_Master_Consolidation_2026.pdf` as the master architecture consolidation for doctrine,
roadmap, risk framing, decisions, and vocabulary. It is a design baseline, not a final product specification. Current
Rust code remains the source of truth for implemented APIs.

At the start of substantial work, classify the task against the master consolidation:

| Question                        | Default guidance                                                                                                                                           |
|---------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Which phase?                    | Default to the V0 recoverable vertical slice unless the task explicitly targets a later phase.                                                             |
| Which engine owns it?           | Map work to a functional engine, then check transverse planes: durability, security, resource governance, policy, observability, and contract integrity.   |
| Which surface?                  | Keep Application, Administration, and HA/DR surfaces separate. Do not mix capabilities across surfaces.                                                    |
| Which decision or risk applies? | Respect locked decisions from the PDF decision register and treat recovery, security-surface bypass, schema drift, and scope explosion as high-risk areas. |

Use this acceptance gate before proposing or implementing features: the feature must be defined, typed, bounded,
observed, versioned, secured, recoverable after crash, explainable, and disableable without corrupting the engine.

## Build, test, and lint

Run commands from the repository root.

```powershell
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```

Useful narrower commands:

```powershell
cargo check --quiet
cargo build --workspace
cargo test -p andromeda-tx --quiet
cargo test -p andromeda-catalog --test catalog_contract_digest -- --nocapture
cargo test -p andromeda-catalog <test_name> -- --nocapture
```

Prototype smoke commands:

```powershell
cargo run -p andromeda-cli -- vertical-v0 --wal "$env:TEMP\andromeda-v0-vertical.wal"
cargo run -p andromeda-cli -- recovery-inspect "$env:TEMP\andromeda-v0-vertical.wal"
cargo run -p andromeda-cli -- protocol-smoke --detail
```

For implementation changes, the required workspace gates are `cargo fmt --all -- --check`, `cargo check --workspace`,
and `cargo test --workspace`. `cargo clippy --workspace --all-targets` is the lint command documented in
`.claude/CLAUDE.md`.

## Architecture

Think in functional engines plus transverse planes. Functional engines own primary behavior. Transverse planes impose
cross-cutting rules and must not become one bypassable meta-engine.

The workspace dependency flow is:

```text
andromeda-core
  -> andromeda-catalog, andromeda-observe, andromeda-proto, andromeda-tx, andromeda-storage
  -> andromeda-srpl, andromeda-quic
  -> andromeda-exec
  -> andromeda-cli
```

Crate roles:

| Crate               | Role                                                                                                                                 |
|---------------------|--------------------------------------------------------------------------------------------------------------------------------------|
| `andromeda-core`    | Shared IDs, errors, type descriptors, hardware profiles, clocks, and digest primitives.                                              |
| `andromeda-catalog` | Catalog objects, `DefinitionBatch`, dependencies, contract hashes, plan cache metadata, statistics, and catalog WAL records.         |
| `andromeda-srpl`    | SRPL lexer, parser, typed AST, semantic binding, cardinality rules, diagnostics, and IR lowering.                                    |
| `andromeda-tx`      | MVCC snapshots, transaction states, row visibility, lifecycle traces, and isolation policy primitives.                               |
| `andromeda-storage` | WAL, LSNs, recovery, page and segment layout, hot/cold placement, manifests, backup, and publication facades.                        |
| `andromeda-proto`   | Protobuf contract types and envelope/frame validation. Protobuf is a boundary format, not the domain model.                          |
| `andromeda-quic`    | QUIC frame codecs, stream/session types, backpressure, sequencing, and RPC dispatch boundaries.                                      |
| `andromeda-observe` | Trace IDs, audit event families, emitters, and principal binding observability.                                                      |
| `andromeda-exec`    | Procedure admission, permission and contract validation, invocation orchestration, rollback, local V0 runtime, and result streaming. |
| `andromeda-cli`     | Prototype command entry points such as `vertical-v0`, `recovery-inspect`, and `protocol-smoke`.                                      |

The V0 request path is:

```text
CLI command
  -> andromeda-exec admission, permission check, and contract validation
  -> SRPL procedure plan
  -> andromeda-tx MVCC snapshot and transaction scope
  -> andromeda-storage WAL write and durable flush
  -> visible commit
  -> ResultStream over QUIC frame policy and Protobuf contract shape
```

Persistent mutation must follow WAL-before-visible-commit semantics. RAM is never truth; the reconstructible source of
truth is a cold snapshot plus WAL. Recovery mounts the manifest and cold snapshot, replays committed durable WAL records
from the required LSN, discards incomplete transactions, rebuilds volatile structures, verifies invariants, then opens
in `FastStart`, `SafeStart`, or `ForensicStart` mode.

Runtime surfaces:

| Surface        | Allowed capability                                                                                                                  | Excluded capability                                                           |
|----------------|-------------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------------------------------------------|
| Application    | `HELLO`, `AUTH`, read contracts, execute Procedures, stream results, return errors, minimal telemetry.                              | Object creation, administration, cluster operations, maintenance, deep debug. |
| Administration | `DefinitionBatch` import, backup/restore, plan inspection, SRPL debug on isolated snapshots, certificate and permission management. | Application traffic mixing and silent audit bypass.                           |
| HA/DR          | WAL shipping, snapshot shipping, quorum, fencing, promotion, health state.                                                          | User Procedure execution and arbitrary administration.                        |

## V0 roadmap

Keep implementation work biased toward the early roadmap:

| Phase   | Scope                                                                                                                                                                        | Exit criterion                                                                    |
|---------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------------------------------------------|
| Phase 0 | Lexicon, catalog object model, type system v0, Procedure contract v0, RPC frame v0, Protobuf schema v0, PageHeader v0, WalRecord v0, Manifest v0, transaction state machine. | Specification artifacts exist with test vectors.                                  |
| Phase 1 | Catalog, minimal SRPL, Transaction Kernel, WAL, Recovery, contractual RPC, ResultStream, Procedure Store trace.                                                              | Crash after commit recovers visible state; crash before commit publishes nothing. |
| Phase 2 | BufferPool, HotStore, cold snapshot publication, atomic manifest switch, WAL replay, basic MVCC, logical checkpoint.                                                         | Recovery can mount snapshot plus WAL and verify invariants.                       |
| Phase 3 | Procedure plan cache, basic cost model, `StatsVersion`, Procedure Store feedback.                                                                                            | Plan decisions are traceable and invalidated correctly.                           |
| Phase 4 | Maps, advanced `StructuredObject`, row/column layout, `RowCountExact` protocol.                                                                                              | StructuredObject batch path supports set-based execution and result metadata.     |
| Phase 5 | mTLS, `UserPrincipal`, certificate registry, permissions, Admin surface, audit ledger, debug snapshot.                                                                       | Denied commands leave traces; debug cannot mutate production.                     |
| Phase 6 | Optional GPU statistics, `ScenarioEvidence`, Predictive Evidence Engine, advanced Procedure Store.                                                                           | Evidence informs the optimizer without overriding policy.                         |

## Project invariants

Preserve these boundaries in code, docs, generated contracts, and examples:

- Application execution goes through cataloged Procedures only. Do not introduce an application-facing ad hoc SQL
  surface.
- SRPL is a strict relational Procedure language, not renamed SQL.
- QUIC is the transport. Protobuf is the contract serialization format. Do not introduce gRPC.
- JSON can be diagnostic output, but it must not become the default runtime wire format.
- Every Procedure contract is typed, versioned, and hashed with a `ContractHash`; reject contract mismatches before
  transaction creation.
- `DefinitionBatch` is the catalog mutation path: parse, canonicalize, dependency graph, dry run, transactional apply,
  new `CatalogVersion`.
- Admission control must happen before transaction creation when session, contract, permission, resource budget, or
  system health state cannot safely accept a Procedure. Refusal must produce a typed error and audit where appropriate.
- GPU work can accelerate analytics or evidence only. It must not participate in commit, WAL, rollback, recovery, or
  other durability-critical paths.
- Critical decisions must be observable after the fact with the appropriate trace or audit family.

## Rust conventions

- All crate roots use `#![forbid(unsafe_code)]`; keep new crate code unsafe-free unless a project decision explicitly
  creates a reviewed low-level exception.
- Keep crate boundaries aligned with engine boundaries. Do not add storage or network dependencies to `andromeda-core`,
  direct network dependencies to `andromeda-catalog`, physical storage dependencies to `andromeda-srpl`, GPU
  dependencies to `andromeda-tx`, Procedure semantics to `andromeda-storage`, or catalog mutation behavior to
  `andromeda-quic`.
- Use explicit error types at engine boundaries and avoid panics in critical paths such as admission, authorization,
  commit, WAL, rollback, recovery, storage publication, security, and protocol validation.
- Treat Protobuf-generated types as boundary contracts. Convert to domain types before applying engine logic.

## Test conventions

Integration tests for external API, contract, and architecture-boundary behavior live under `crates/<crate>/tests/`.
They are named by contract or gate, such as `catalog_contract_digest.rs`, `runtime_contracts.rs`,
`cancel_backpressure_contract.rs`, `wal_scan_recovery.rs`, and `v0_recoverable_vertical.rs`.

Inline unit tests cover module-local behavior. Binary or recovered-byte parsers need property or fuzz coverage as the
implementation matures, especially SRPL parsing, Protobuf envelope decoding, QUIC frame headers, WAL record parsing,
manifests, page headers/trailers, and `StructuredObject` batch readers.

Critical V0 regressions are release blockers: recovery failure, visibility violation, contract mismatch acceptance,
audit omission on security or administration operations, panic in critical paths, silent data corruption, and
non-reproducible crash tests.

## Errors and completion

Preserve the PDF error model boundaries:

| Error family                                                       | Transaction effect                                                                   |
|--------------------------------------------------------------------|--------------------------------------------------------------------------------------|
| Protocol, authentication, authorization, and early contract errors | No transaction should be created.                                                    |
| Late contract errors                                               | No transaction or rollback if detection happens after work starts.                   |
| Semantic errors during definition                                  | Catalog change rollback.                                                             |
| Execution and resource errors                                      | Controlled error, cancellation, or rollback according to state and Procedure policy. |
| Transaction and storage errors                                     | Rollback, fail-stop, or transition to recovery/forensic mode as appropriate.         |

Every invocation must end with a terminal completion state such as `Committed`, `RolledBack`, `FailedBeforeTransaction`,
`Cancelled`, `Poisoned`, `PermissionDenied`, `ContractRejected`, or `SystemUnavailable`.

## Documentation and terminology

Use American English and Microsoft documentation style: short headings, short paragraphs, active voice, tables for
matrices, and ordered steps for procedures.

Use the project vocabulary consistently:

| Use              | Avoid                       |
|------------------|-----------------------------|
| Procedure        | Stored procedure, query     |
| Invocation       | Execution, call             |
| Procedure Store  | Query Store                 |
| Map              | View, materialized view     |
| StructuredObject | DTO, table-valued parameter |
| DefinitionBatch  | Migration script, DDL       |
| RowCountExact    | COUNT metadata              |

For durable architecture, contract, catalog, WAL, storage format, SRPL, security, or protocol changes, update or create
the relevant decision record or instruction artifact instead of leaving the decision only in code comments.
