# andromeda-proto-wire

## Purpose

`andromeda-proto-wire` owns wire-level protobuf integration helpers that can stay independent
from the historical `andromeda-proto` compatibility facade.

## Scope

- Owns generated or generated-like protobuf adapter boundaries for external interfaces.
- Holds typed DTO mappings used by protocol-facing clients.
- Defines deterministic protobuf codec shims, descriptor hashing helpers, and generated
  metadata bound checks used by protocol-facing projections.

## Non-goals

- Own transport negotiation, session lifecycle, or QUIC socket behavior.
- Own Procedure execution, transaction logic, WAL recovery, or catalog persistence.
- Replace stable protocol contracts owned by `andromeda-rpc-protocol`.

## Procedure

1. Keep the crate wire-focused and isolated from runtime behavior.
2. Keep message types and mappings explicit and deterministic.
3. Defer all transport and RPC orchestration to dedicated crates.

## Validation

- When active in the workspace, validate with normal Rust checks:
  - `cargo fmt --all --check`
  - `cargo check -p andromeda-proto-wire --all-targets`

## Troubleshooting

- If proto types conflict with existing runtime contracts, move shared boundaries to a higher-level protocol contract crate.
- If generation tooling is missing, keep generated message ownership in `andromeda-proto`
  and move only compile-safe wire helpers here.

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/andromeda-proto`
- `crates/andromeda-rpc-protocol`
