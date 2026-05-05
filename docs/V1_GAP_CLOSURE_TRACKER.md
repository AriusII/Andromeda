# V1 Gap Closure Tracker

Date: 2026-05-05

This tracker converts the implementation reality matrix into concrete V1
closure work. It is intentionally kept under `docs/` so working delivery notes
do not repopulate the repository root.

## Current Closure Waves

| Wave | Area | Goal | Expected evidence |
| --- | --- | --- | --- |
| V1-A | Benchmark runner | Make `andromeda-cli benchmark run` call a bounded `andromeda-bench` runner instead of stopping at scaffold validation. | CLI run test, diagnostic JSON evidence test, no runtime JSON wire dependency. |
| V1-B | Durable audit sink | Add a minimal restart-surviving audit sink or replayable journal contract. | Append/reopen/query test, fail-closed validation, no secret leakage. |
| V1-C | Catalog/proto statuses | Lock catalog manifest resolution statuses and validation policy. | Proto/schema governance tests and catalog status mapping tests. |
| V1-D | Storage format manifest | Source pre-redo storage format fingerprints from durable manifest evidence. | Manifest validation test and pre-redo gate test using manifest-derived fingerprints. |
| V1-E | TX recovery replay | Rebuild transaction terminal status from transaction-local replay records without storage coupling. | Duplicate/conflict/incomplete replay tests. |
| V1-F | QUIC pool/retry contract | Add runtime-free connection pool and retry admission contracts. | Pool key, idle/unhealthy eviction, idempotent retry policy tests. |

## Closure Criteria

| Gap | Closed when |
| --- | --- |
| CLI scaffold vs runner | `benchmark run` validates, executes a bounded in-process runner, and emits deterministic evidence. |
| Audit contract vs durable sink | A sink implementation can append records, be reopened, replay reports, and reject invalid/security-sensitive evidence. |
| Catalog status drift | Status enum values, validation, and docs/tests agree on malformed, unsupported, unavailable, auth, and internal failures. |
| Recovery format source | Recovery startup can reject/accept based on manifest-owned storage format fingerprints, not caller-supplied ad hoc vectors alone. |
| TX status recovery | Commit/rollback terminal records can be replayed idempotently and conflicting terminal evidence is rejected before visibility. |
| QUIC reconnection | Reconnect/pool policy is deterministic, bounded, and separates retryable idempotent calls from unsafe calls. |

## Non-Goals For These Waves

- No full B-Tree page/WAL redo implementation in this wave.
- No full Heap CRC/torn-write implementation in this wave.
- No real Quinn socket pooling in this wave; only runtime-free contracts.
- No durable audit database/query engine; the first target is replayable append evidence.
- No storage dependency reintroduced into `andromeda-tx`.

## Residual V1 Risks To Recheck After The Wave

1. `andromeda-cli` may still have admin commands backed by mocks after benchmark wiring is fixed.
2. `SrplProcedureDispatcher::result_metadata_for_plan` still cannot extract metadata from plan-only input.
3. B-Tree tests now cover in-memory behavior but not persisted node split/recovery.
4. Heap format tests cover layout corruption but not full page checksum/torn-write proof.
5. Audit replay may still need ordering and query integration beyond the first durable sink.
6. QUIC reconnect contracts still need real runtime integration and certificate continuity checks.
