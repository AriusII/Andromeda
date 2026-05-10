# Specification: ResultStream v0

> **Status:** Normative V0 specification
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers
> **Language:** American English
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article

- Define the purpose and scope of `ResultStream v0`.
- State the required stream structures.
- State invariants, errors, security, recovery, tests, and rejection criteria.

## Purpose

Define the typed result stream contract for Procedure execution.

## Scope

This specification applies to V0 Procedure results exposed through the custom RPC protocol. It defines metadata order, payload frames, completion frames, typed errors, and stream roles.

## Non-goals

- It does not define a final production transport implementation.
- It does not authorize gRPC, JSON-native, or ad hoc SQL result formats.
- It does not make result payloads a durable source of truth.

## Data structures

| Structure | Required role |
|---|---|
| `ResultStream` | Ordered stream for one Procedure invocation. |
| `ResultMetadata` | Contract-bound metadata that must precede payload. |
| `ResultPayloadFrame` | Length-prefixed typed payload frame. |
| `ResultCompletion` | Terminal success, rollback, rejection, cancellation, or error frame. |
| `StreamRole` | Application, Administration, HA/DR, or internal role boundary. |
| `ResultStreamError` | Typed stream error family. |
| `BackpressureDecision` | Bounded slow, spool, shrink, cancel, or reject decision. |
| `ResultStreamDescriptor` | Metadata declaration for each named result stream, including shape and row-count policy. |
| `ResultCompletionPolicy` | Explicit policy: row batch required, zero-row completion allowed, or mutation-only. |
| `InvocationCorrelation` | Stable request, session, trace, contract, catalog, invocation, stats, and policy version binding. |
| `ResponseIndex` | Gap-free sequence ordinal for generated invocation responses. |
| `BatchIndex` | Gap-free per-result-stream ordinal for payload batches. |
| `ResultRowCountSummary` | Completion summary for every declared result stream. |
| `StructuredObjectLayout` | RowMajor, ColumnMajor, or Hybrid payload layout descriptor for structured result bytes. |

## Invariants

- Result metadata precedes every payload frame.
- Payload frames reference the active `ProcedureContract`, `ContractHash`, and `CatalogVersion`.
- A stream has exactly one terminal `ResultCompletion`.
- Stream role is explicit and cannot be inferred from transport port alone.
- Backpressure decisions are bounded, observable, and never mutate durable truth.
- All frames in one ResultStream keep the same `RequestId`, `SessionId`, optional `TransactionId`, `ContractHash`, `CatalogVersion`, `InvocationId` when present, `StatsVersion`, and `PolicyVersion`.
- `ResponseIndex` is gap-free across invocation responses, and `BatchIndex` is gap-free within each declared result stream.
- `ResultCompletion` summarizes every declared result stream exactly once.
- Row-count exactness and maximums are declared in metadata and cannot be introduced later by batch or completion frames.
- Mutation-only completion must not declare or emit row result streams.

## Serialization

- Network-visible stream frames use canonical encoding under `SPEC_RPC_FRAME_V0.md`.
- Frame headers use fixed-width integer fields and declare payload length before payload.
- Payloads are typed by contract layout; JSON-native payloads are not a V0 protocol format.
- Native Rust layout must not be sent over the wire.
- Metadata, batch, completion, and error payloads are custom Andromeda Protobuf messages inside RPC frames; they are not gRPC messages, REST responses, or JSON-native documents.
- Every payload batch declares its byte length through the enclosing RPC frame and is validated against frame and admitted stream budgets before allocation.
- Structured result bytes carry layout and batch descriptors before raw values are interpreted.

### ResultStream v0 frame order

The only successful row-producing sequence is:

```text
RpcMetadata -> RpcBatch+ -> RpcCompletion
```

The only successful zero-row sequence is:

```text
RpcMetadata(policy = ZeroRowCompletionAllowed) -> RpcCompletion
```

The only successful mutation-only sequence is:

```text
RpcMetadata(policy = MutationOnly, declared result streams = empty) -> RpcCompletion
```

The terminal error sequence is:

```text
RpcMetadata? -> Error
```

`Error` is terminal and must use the same invocation correlation when correlation exists. A pre-metadata protocol error is allowed only when metadata cannot be decoded; it must be emitted on the diagnostic path, not as an untyped result payload.

### ResultMetadata v0

`ResultMetadata` must be the first result-stream frame and must include:

| Field | Requirement |
|---|---|
| `InvocationCorrelation` | Stable correlation binding with nonzero request/session ids, `ContractHash`, `CatalogVersion`, `StatsVersion`, and `PolicyVersion`. |
| `ProcedureId` or Procedure name | Required for Procedure-bound results. |
| `ContractHash` | Required and equal to the admitted Procedure contract. |
| `CatalogVersion` | Required and nonzero. |
| `ResultCompletionPolicy` | Required and explicit. |
| `ResultStreamDescriptor[]` | Declares every row-producing result stream by unique name. |
| Column descriptors | Required for each row-producing stream; include name, type, cardinality, absence policy, and order. |
| `StructuredObjectLayout` | `RowMajor`, `ColumnMajor`, or `Hybrid`, with batch layout descriptors. |
| Row-count policy | `ExactRequired`, `MaxBounded`, or `UnknownBounded`; exact and max values are metadata-owned. |

### ResultPayloadFrame v0

Each `RpcBatch` must:

| Rule | Requirement |
|---|---|
| Result name | Match exactly one declared `ResultStreamDescriptor`. |
| `BatchIndex` | Start at `0` and increase by one with no gaps per result stream. |
| Terminal batch marker | Required before completion when rows were emitted for that stream. |
| Row counts | `rows_emitted` accumulates without overflow and does not exceed metadata exact or max bounds. |
| `row_count_exact` | Must match metadata when metadata declares exact count; must be absent when metadata did not declare it. |
| Payload layout | Must match the declared `StructuredObjectLayout` and column descriptors. |
| Payload bytes | Must be bounded before allocation and are never interpreted as native Rust layout. |

### ResultCompletion v0

`ResultCompletion` is the only successful terminal frame and must include:

| Field | Requirement |
|---|---|
| Completion kind | Committed, rolled back, rejected, cancelled, or failed. |
| Transaction outcome | Required and typed when the Procedure had transaction scope. |
| `ResultRowCountSummary[]` | One summary per declared result stream, unique by result name. |
| `rows_emitted` | Must equal the sum observed in batches for that stream. |
| `row_count_exact` | Must match metadata exact count and cannot appear if metadata omitted exact count. |
| Durable evidence | Required for mutation results that became visible; references the owning WAL/transaction specs. |

Completion before metadata, repeated completion, payload after completion, batch after terminal batch, or completion without a required batch is invalid.

## State transitions

```text
Created -> MetadataSent -> PayloadOpen -> Completed
Created -> MetadataSent -> Rejected
Created -> Cancelled
PayloadOpen -> Cancelled
PayloadOpen -> Failed
```

Invalid transitions return typed stream errors and emit trace evidence.

## Error model

| Error family | Use |
|---|---|
| ContractError | Metadata does not match active Procedure contract or payload layout. |
| ResourceError | Backpressure, quota, timeout, or bounded spool failure. |
| TransactionError | Rollback or transaction-scoped execution failure. |
| PermissionError | Stream role or surface scope mismatch. |
| SystemError | Runtime failure requiring cancellation or forensic evidence. |

### Typed ResultStream rejection mapping

| Rejection | Error family |
|---|---|
| Payload before metadata, repeated metadata, batch after completion, second terminal frame, context drift, payload-kind spoofing, response index gap, batch index gap, unknown completion policy, or JSON-native payload | `ProtocolError` |
| Missing contract hash, missing catalog version, missing policy version, undeclared result name, missing completion policy, row-count exactness introduced after metadata, row-count drift, or mutation-only metadata with result streams | `ContractError` |
| Oversized batch, allocation before bounds check, row-count overflow, admitted stream budget exceeded, or bounded spool failure | `ResourceError` |
| Stream role mismatch or Administration/HA/DR result on Application surface | `PermissionError` |
| Transaction-scoped completion without required transaction outcome or durable mutation evidence | `TransactionError` |

## Security model

Result streams inherit the admitted surface, principal, permissions, policy version, and Procedure contract binding. Administration or HA/DR results must not appear on the Application Surface.

## Observability

At minimum, implementations must emit trace evidence with:

```text
TraceId
InvocationId
ProcedureId
ContractHash
CatalogVersion
StreamRole
CompletionKind
ErrorKind when applicable
```

### Trace evidence requirements

Every ResultStream rejection emits bounded evidence with `TraceId`, `InvocationId` when assigned, `RequestId`, `SessionId`, `ContractHash` when decoded, `CatalogVersion` when decoded, `PolicyVersion` when decoded, `StreamRole`, `ResultStreamError`, and the rejection reason.

For pre-metadata failures, unavailable contract, catalog, policy, or principal fields must be represented as explicitly unavailable rather than invented. Payload bytes and row values are not trace evidence.

## Recovery behavior

ResultStream output is not durable system truth. Recovery reconstructs durable state from snapshot plus WAL, then may emit a new stream. A partially sent stream must be completed, cancelled, or failed from observable state.

## Compatibility

| Change | Default status |
|---|---|
| Add optional metadata field with explicit default | Additive |
| Add required metadata field | Breaking |
| Change frame order | Breaking |
| Change payload layout | Breaking unless tied to a new ContractHash |
| Change completion semantics | Breaking |

## Tests

- metadata-before-payload tests.
- single-completion tests.
- oversized payload rejection tests.
- role mismatch rejection tests.
- cancellation and backpressure tests.
- row batch required, zero-row allowed, and mutation-only policy tests.
- stable invocation correlation tests.
- gap-free `ResponseIndex` and `BatchIndex` tests.
- declared result name tests.
- row-count exact/max drift tests.
- terminal batch before completion tests.
- payload-kind spoofing tests.
- typed error and rejection trace evidence tests.

## Rejection criteria

- Reject `payload before metadata`.
- Reject `metadata repeated after metadata`.
- Reject `completion before metadata`.
- Reject `completion without required batch`.
- Reject `second completion frame`.
- Reject `payload after completion`.
- Reject `batch after terminal batch`.
- Reject `gap in ResultStream response index`.
- Reject `gap in ResultStream batch index`.
- Reject `ResultStream invocation correlation drift`.
- Reject `ResultStream payload kind spoofing`.
- Reject `undeclared ResultStream result name`.
- Reject `row_count_exact introduced after metadata`.
- Reject `row_count_exact mismatch`.
- Reject `rows_emitted exceeds row_count_max`.
- Reject `completion missing declared result summary`.
- Reject `mutation-only ResultStream with declared row stream`.
- Reject `ResultStream payload allocation before bounds check`.
- Reject `stream role inferred from transport only`.
- Reject `Administration or HA/DR result on Application Surface`.
- Reject `JSON-native result payload`.
- Reject `native Rust layout result payload`.

## Acceptance summary

Owner: Person 12 owns the ResultStream RPC sequencing contract with implementation evidence from `crates/andromeda-rpc-protocol`, typed Protobuf validation evidence from `crates/andromeda-rpc-codec` and `crates/andromeda-proto-wire`, and cross-spec contract evidence from `SPEC_PROCEDURE_CONTRACT_V0.md` and `SPEC_RPC_FRAME_V0.md`.

Evidence: acceptance requires retained tests proving `RpcMetadata -> RpcBatch* -> RpcCompletion` ordering, explicit zero-row and mutation-only policies, stable invocation correlation, gap-free `ResponseIndex` and `BatchIndex`, declared result names, metadata-owned row-count exact/max policy, one completion summary per declared stream, terminal batch readiness, bounded payload allocation, `StreamRole` and `SurfaceScope` separation, typed errors, and no JSON-native or native Rust result payloads.

Reject: acceptance is denied for any implementation that accepts payload before metadata, repeated metadata, completion before metadata, second completion, payload after completion, batch after terminal batch, correlation drift, index gaps, undeclared result names, row-count drift, mutation-only row streams, payload-kind spoofing, unbounded payload allocation, stream role inferred from transport only, Administration or HA/DR results on Application surface, JSON-native result payloads, or native Rust layout result payloads.
