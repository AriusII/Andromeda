# Vertical Slice V0

## Steps

1. Define minimal catalog objects.
2. Define SRPL minimal procedure.
3. Generate Protobuf contracts.
4. Map to QUIC frames.
5. Implement WAL-visible commit path.
6. Return ResultStream.
7. Record Procedure Store trace.
8. Run crash/recovery test.
9. Run verification matrix gates for workspace, doctrine, SRPL, catalog, protocol, WAL/recovery, storage, security, and
   observability as applicable.

## Required verification

- Workspace commands:
    - `cargo fmt --all -- --check`
    - `cargo check --workspace`
    - `cargo test --workspace`
- Doctrine scans from `instructions\QUALITY_GATES.md`, including gRPC, SQL native surface, runtime JSON default, unsafe
  drift, and GPU critical-path drift checks.
- Procedure contract identity, contract hash, and catalog version mismatch rejection before transaction creation.
- Authorization denial before transaction creation with audit evidence.
- WAL-before-visible-commit with durable LSN evidence.
- Crash/recovery scenario that is deterministic and reproducible.
- Protocol result sequencing through QUIC frame policy and Protobuf contract shape without gRPC.
- Observability traces for contract validation, authorization, WAL append/flush, commit visibility, rollback/recovery,
  catalog mutation, protocol rejection, and corruption boundary.

## Release blockers

- Recovery failure.
- Visibility violation.
- Contract mismatch acceptance.
- Audit omission.
- Panic in critical path.
- Silent corruption.
- Non-reproducible crash test.

## Output

- Summary
- Evidence
- Verification matrix areas and owners
- Commands run
- Release blocker decision
- Decision or recommendation
- Risks
- Open questions
