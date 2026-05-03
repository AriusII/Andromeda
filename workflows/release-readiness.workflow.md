# Release Readiness

## Steps

1. Collect changed artifacts.
2. Map changed artifacts to the verification matrix areas in `instructions\QUALITY_GATES.md`.
3. Run doctrine gate and classify every match as approved reference, fixture, or violation.
4. Run mandatory workspace commands:
    - `cargo fmt --all -- --check`
    - `cargo check --workspace`
    - `cargo test --workspace`
5. Run affected lane gates for SRPL, catalog, protocol, WAL/recovery, storage, security, and observability.
6. Check release blockers:
    - recovery failure;
    - visibility violation;
    - contract mismatch acceptance;
    - audit omission;
    - panic in critical path;
    - silent corruption;
    - non-reproducible crash test.
7. Run documentation gate.
8. Update risks.
9. Approve or block.

## Doctrine scans

Use the scan set in `instructions\QUALITY_GATES.md` for Procedure-only execution, SRPL boundaries, QUIC, Protobuf
without
gRPC, no SQL native surface, no runtime JSON default, no unsafe drift, and no GPU critical-path drift.

## Output

- Summary
- Evidence
- Verification matrix areas and owners
- Commands run
- Release blocker decision
- Decision or recommendation
- Risks
- Open questions
