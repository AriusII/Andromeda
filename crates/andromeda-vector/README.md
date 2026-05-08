# andromeda-vector

## Purpose

`andromeda-vector` owns advisory vector-output validation for Andromeda analytical work.

## Scope

- Own `VectorAdvisoryRequest`, `VectorAdvisoryKind`, and advisory validation.
- Require independent source truth, bounded candidates, and advisory-only status.
- Reject commit, WAL append, rollback, recovery, MVCC visibility, catalog publication, and security-critical paths.
- Keep vector evidence limited to ranking, pruning, or suggesting analytical candidates.

## Non-goals

- Do not own vector indexes, vector storage, model inference runtimes, commit visibility, WAL ordering, rollback, recovery, catalog publication, or security-critical behavior.
- Do not make vector runtime behavior mandatory.
- Do not treat vector output as durable, catalog, transaction, recovery, or security truth.
- Do not introduce implicit SQL or gRPC runtime surfaces.

## Prerequisites

- Callers provide independent source truth before using vector evidence.
- Callers keep vector advisory output outside C5 truth paths.

## Procedure

1. Build a `VectorAdvisoryRequest` for an advisory analytical pipeline.
2. Declare independent source truth.
3. Declare bounded candidate behavior.
4. Mark the request advisory-only.
5. Call `validate_vector_advisory` before using vector evidence.

## Validation

- Inspect `src/lib.rs` for advisory-only validation and C5 rejection.
- When validating by command, use `cargo check -p andromeda-vector --all-targets`.

## Troubleshooting

- If this crate touches C5 truth paths, remove that integration and keep vector evidence outside the path.
- If source truth or candidate bounds are absent, reject the vector advisory request.

## References

- `AGENTS.md`
- `crates/README.md`
- `crates/andromeda-hardware/README.md`
