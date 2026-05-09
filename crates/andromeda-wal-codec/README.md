# andromeda-wal-codec

## Purpose

`andromeda-wal-codec` owns the raw WAL binary frame codec and bounded scan helpers.

## Scope

- Define little-endian WAL frame constants, header validation, and raw frame encode/decode behavior.
- Provide the bounded scan loop used by typed WAL owner wrappers.
- Keep byte layout concerns separate from durable replay, checkpoint, and recovery policy.

## Non-goals

- Do not own WAL record semantics, scheduling, fsync behavior, append orchestration, replay policy, checkpointing, recovery flow control, storage engines, manifest publication, or catalog durability policy.
- Do not replace durable WAL logic implemented in `andromeda-wal`.

## Prerequisites

- Owner crates provide typed WAL record semantics and validation.
- Callers treat scan results as codec evidence, not recovery policy.
- Persistent format changes require explicit versioning and owner-crate validation.

## Procedure

1. Keep codec primitives deterministic, explicit, and isolated from runtime side effects.
2. Keep layout helpers versioned and named by purpose.
3. Delegate replay, checkpointing, and crash recovery policy to runtime-owned crates.

## Validation

- Inspect `src/lib.rs` for explicit byte-format behavior and absence of replay or fsync ownership.
- When validating by command, use `cargo check -p andromeda-wal-codec --all-targets`.

## Troubleshooting

- If format compatibility breaks, align record versions with `andromeda-wal` and add migration notes.
- If this crate must parse external formats, isolate adapters behind narrow modules.

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/andromeda-wal`
- `crates/andromeda-wal-codec/src/lib.rs`
