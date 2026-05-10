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

## Invariants

- Result metadata precedes every payload frame.
- Payload frames reference the active `ProcedureContract`, `ContractHash`, and `CatalogVersion`.
- A stream has exactly one terminal `ResultCompletion`.
- Stream role is explicit and cannot be inferred from transport port alone.
- Backpressure decisions are bounded, observable, and never mutate durable truth.

## Serialization

- Network-visible stream frames use canonical encoding under `SPEC_RPC_FRAME_V0.md`.
- Frame headers use fixed-width integer fields and declare payload length before payload.
- Payloads are typed by contract layout; JSON-native payloads are not a V0 protocol format.
- Native Rust layout must not be sent over the wire.

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

## Rejection criteria

- Reject `payload before metadata`.
- Reject `second completion frame`.
- Reject `stream role inferred from transport only`.
- Reject `JSON-native result payload`.
- Reject `native Rust layout result payload`.

## Acceptance summary

This specification is acceptable when RPC, execution, and result-stream crates can prove metadata ordering, typed completion, role separation, bounded payloads, and non-durable result semantics.
