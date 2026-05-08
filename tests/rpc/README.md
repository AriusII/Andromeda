# RPC Test Index

## Purpose

This directory is a documentation index for RPC and transport validation. It does not own an executable Rust harness.

Executable RPC tests remain with the crates that own Procedure contracts, protocol frames, QUIC transport behavior, ResultStream behavior, and backpressure behavior.

## Scope

Use this index for roadmap entries that refer to `tests/rpc`.

Entries should name the owning crate, protocol surface, compatibility risk, expected frame behavior, and validation command.

## Test destination

| Scenario type | Destination |
| --- | --- |
| Procedure contract projection, typed envelope, metadata, or payload shape | The owning contract, RPC codec, protocol, or executor crate's tests. |
| Frame codec, ordering, compatibility, or backpressure behavior | The owning RPC protocol, ResultStream, or transport crate's tests. |
| QUIC session, admission, reconnect, certificate, or transport behavior | The owning QUIC runtime or transport crate's tests. |
| End-to-end remote Procedure invocation | The executor or client-facing crate that owns the full path being asserted. |
| Fuzz-discovered wire-format or codec defect | A deterministic crate-local regression; keep fuzz target and corpus metadata in `fuzz/` and index them through `tests/fuzzing/`. |
| Root roadmap RPC coverage | This README, as an index entry that points to the owning crate command. |

## Non-goals

- Do not introduce gRPC or JSON runtime protocols.
- Do not expose Administration or HA/DR behavior through the Application Surface.
- Do not bypass typed Procedure contracts.
- Do not create root-level RPC harnesses without an explicit future work order.
- Do not duplicate crate-local protocol tests, fuzz targets, seed corpora, or generated fixtures in this directory.

## Prerequisites

- Review `tests/README.md`.
- Identify the contract, codec, transport, or executor crate that owns the behavior.

## Procedure

1. Map the RPC scenario to the owning crate.
2. Keep executable protocol and transport tests in that crate.
3. Convert fuzz-discovered wire-format failures into deterministic codec or protocol regressions before citing them here.
4. Record the expected metadata, payload, error, ordering, or backpressure evidence in this index when the scenario is ready.

## Validation

For this documentation index, run:

```powershell
rg -n "Purpose|Scope|Validation" tests/rpc
```

Runtime validation belongs to the owning RPC, protocol, QUIC, executor, or client crate test command.

## Troubleshooting

If a scenario crosses transport and Procedure execution, split the evidence by owning crate and keep this directory as the index only.

## References

- `tests/README.md`
- `docs/codex/rust-critical-quality-gates.md`
- `documentations/testing/step-11-validation-matrix.md`
