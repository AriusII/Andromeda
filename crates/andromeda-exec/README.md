# andromeda-exec

## Purpose

`andromeda-exec` owns execution orchestration for typed, cataloged Procedures. It connects admission, procedure dispatch, SRPL dispatch handoff, local runtime transactions, result validation, result-stream completion, retry routing, audit traces, and completion recovery evidence.

This crate must never create an application-facing ad hoc SQL surface. Application work enters through typed Procedure contracts, catalog bindings, permission-checked dispatch, and durable transaction evidence.

## Scope

This crate owns:

- Procedure admission, pre-transaction validation, and contract-binding checks.
- Surface gates that admit application Procedure execution and keep Administration and HA/DR capabilities off the Application Surface.
- Local and remote Procedure dispatch boundaries, including SRPL dispatch handoff.
- Runtime transaction orchestration over `andromeda-tx`, `andromeda-storage`, and cataloged Procedure metadata.
- Result metadata extraction, bounded ResultStream behavior, completion mapping, and durable terminal evidence.
- Retry classification, timeout and deadlock routing, durable rollback fences, and audit ledger events.
- Execution-facing typed errors through `AndromedaResult`, stable error kinds, and explicit recovery/terminal evidence structs.

## Public API compatibility

- `andromeda_exec::retry` is a stable re-export surface for retry types, backed by `andromeda-retry`.
- `andromeda_exec::traces` is a stable re-export surface for audit trace types, backed by `andromeda-execution-trace`.
- `andromeda_exec::{SrplExecutionAdapter, SrplTypedEnvironment, FieldValue, StructuredObject}` remains a compatibility re-export surface for concrete SRPL adapter behavior now owned by `andromeda-execution`.
- Root re-exports currently include `ErrorRetryability`, `RetryAttempt`, `RetryDecision`, `RetryPolicy`, `AuditLedger`, `InMemoryAuditLedger`, and `InvocationTraceEvent`.

## Non-goals

This crate does not own:

- Catalog object storage, DefinitionBatch semantics, plan-cache identity, or statistics publication.
- WAL byte formats, FileWal ownership, page codecs, heap codecs, B+Tree durable formats, or recovery replay implementation.
- Transaction status authority outside the transaction crate boundary.
- Application-facing SQL, raw text query dispatch, dynamic predicates, or shape-shifting returns.
- QUIC protocol byte contracts or authorization policy stores, except through typed boundary calls.
- GPU, benchmark, analytics, or learned-model output as execution, commit, recovery, or security truth.

## Prerequisites

Before changing this crate, confirm that the change respects these requirements:

- A dispatch request carries a Procedure contract reference and full `ProcedureContractBinding` before handler execution.
- Surface authorization proves the caller is on the allowed plane for Procedure execution.
- Transaction completion includes durable WAL LSN evidence before committed or rolled-back result metadata is emitted.
- Result stream completion publishes metadata before payload completion and rejects zero or missing durable LSN evidence for terminal states.
- Timeout, deadlock, and retry routing produce one terminal state with a durable WAL fence; they must not create conflicting commit and rollback outcomes.
- Error paths are typed and classify admission, permission, dispatch, transaction, retry, and completion failures.

## Procedure

1. Identify whether the change affects admission, surface gating, dispatch, SRPL adapters, local runtime, result streams, retry routing, or completion recovery.
2. Keep Procedure contracts mandatory. Do not add a raw SQL string, dynamic procedure shape, or unbound handler path as a shortcut.
3. Preserve pre-transaction checks. Validate contract hash, catalog version, invocation identity, and permission evidence before creating transaction state.
4. Preserve durable completion ordering. A committed result, rolled-back result, product-state publication, or terminal audit event must carry nonzero durable WAL evidence.
5. Keep rollback and retry routing single-terminal. If a timeout or deadlock is routed to rollback, make the durable rollback fence explicit and reject conflicts during recovery.
6. Add targeted tests for the affected boundary. Result-stream and runtime changes need metadata, backpressure, completion, rollback, and recovery evidence coverage.

## Validation

Recommended execution gates:

```powershell
cargo test -p andromeda-exec --test surface_gate_contract -- --nocapture
cargo test -p andromeda-execution --test srpl_adapter_contract -- --nocapture
cargo test -p andromeda-exec --test runtime_contract -- --nocapture
cargo test -p andromeda-exec --test result_stream_backpressure -- --nocapture
cargo test -p andromeda-exec --test timeout_deadlock_routing_contract -- --nocapture
cargo test -p andromeda-exec --test retry_semantics -- --nocapture
cargo test -p andromeda-exec --test v0_vertical_e2e -- --nocapture
```

If a change crosses into storage, WAL, transaction, catalog, security, or RPC behavior, add the owning crate's contract tests. For crash-sensitive execution changes, include recovery visibility tests that prove completions reconstruct from durable WAL evidence rather than transient runtime state. Add property tests for bounded state transitions and fuzz coverage for malformed protocol, ResultStream, or adapter inputs when the changed boundary accepts external bytes.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| Procedure dispatch is rejected before transaction creation | Verify the contract binding, contract hash, catalog version, invocation id, and permission evidence. |
| A terminal result stream completion fails validation | Check that terminal transaction state and nonzero durable WAL LSN evidence are present. |
| A timeout or deadlock produces conflicting state | Inspect retry routing, rollback fence evidence, and completion recovery reconciliation. |
| Application Surface admits a non-Procedure action | Tighten `SurfacePlaneAuthorizer` and permission scope validation. |
| ProductStock state becomes visible too early | Verify durable commit evidence and heap redo evidence match before publication. |

## References

- `src/lib.rs`
- `src/admission.rs`
- `src/surface_gate.rs`
- `src/dispatch/`
- `src/local/`
- `src/result.rs`
- `src/result_stream.rs`
- `src/retry.rs`
- `src/services/transaction_error_routing.rs`
- `src/services/completion/recovery.rs`
- `tests/surface_gate_contract.rs`
- `tests/runtime_contract.rs`
- `tests/result_stream_backpressure.rs`
- `tests/timeout_deadlock_routing_contract.rs`
- `tests/v0_vertical_e2e.rs`
