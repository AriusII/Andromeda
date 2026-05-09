# andromeda-protocol

## Purpose

`andromeda-protocol` reserves a runtime-free protocol contract boundary shared by transport and Procedure-facing layers.

## Scope

- Own protocol-level marker and contract definitions that are not specific to QUIC runtime wiring.
- Keep protocol-facing definitions explicit, typed, and versionable.
- Provide a stable lower boundary for future protocol compatibility work.

## Non-goals

- Do not own QUIC listeners, TLS, sockets, async runtime behavior, persistence, WAL, recovery, or catalog publication.
- Do not introduce gRPC, ad hoc SQL, JSON runtime defaults, or untyped protocol behavior.
- Do not duplicate byte-level frame ownership from `andromeda-rpc-protocol` or payload codec ownership from `andromeda-rpc-codec`.
- Do not depend on the generated Protobuf wrapper crate; compatibility exports here must come from runtime-free owner crates such as `andromeda-proto-wire`.

## Prerequisites

- Protocol definitions have an identified caller and compatibility requirement.
- Runtime behavior remains in transport or execution crates.

## Procedure

1. Add only runtime-free protocol contracts or markers here.
2. Keep byte layouts, stream roles, and frame sequencing in their owning protocol or codec crates.
3. Document compatibility effects before changing public protocol definitions.

## Validation

- Inspect `src/lib.rs` for runtime-free scope.
- When concrete APIs are added, run `cargo check -p andromeda-protocol --all-targets`.

## Troubleshooting

- If a definition needs sockets or TLS, move it to the transport owner.
- If a definition needs WAL, storage, or catalog authority, move it to the owning durable subsystem.

## References

- `AGENTS.md`
- `crates/README.md`
- `crates/andromeda-rpc-protocol/README.md`
