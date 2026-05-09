# andromeda-quic-runtime-quinn

## Purpose

`andromeda-quic-runtime-quinn` owns the concrete Quinn-backed QUIC runtime adapter for Andromeda.

## Scope

- Own Quinn, Rustls, Tokio, socket, and TLS wiring.
- Adapt Quinn connections and streams to runtime-free Andromeda transport contracts.
- Keep mTLS certificate extraction, 0-RTT disabling, retry admission evidence, and network integration tests outside `andromeda-quic`.

## Non-goals

- Define core protocol frame, contract, dispatch, or manifest semantics.
- Implement durable WAL, recovery, transaction, or catalog logic.
- Expose application-facing SQL or untyped dispatch surfaces.

## Prerequisites

- Rust 2024 workspace baseline.
- `andromeda-quic` owns runtime-free transport and reconnect contracts.
- `andromeda-rpc-protocol` owns frame bytes and stream roles.
- `andromeda-rpc-codec` owns typed payload envelope validation.

## Procedure

1. Keep concrete runtime dependencies confined to this crate.
2. Reexport only typed runtime adapter interfaces needed by integration tests and callers.
3. Keep 0-RTT disabled unless a future decision record changes the doctrine.

## Validation

- Validate with:
  - `cargo check -p andromeda-quic-runtime-quinn --all-targets --all-features`
  - `cargo test -p andromeda-quic-runtime-quinn --test reconnect_quinn_admission_contract`
  - `cargo test -p andromeda-quic-runtime-quinn --test real_quinn_network`

## Troubleshooting

- If `andromeda-quic` starts requiring Quinn, move that code back into this crate.
- If 0-RTT becomes enabled, require an explicit decision record and update doctrine tests before code changes.
- If a runtime test is flaky, fix the deterministic timeout or transport setup; do not normalize flakiness through retries.

## References

- `AGENTS.md`
- `crates/AGENTS.md`
