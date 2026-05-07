# Rust Crates Instructions

These instructions apply to Rust crate work under `crates/`.

## Rules

- One crate must have one primary responsibility.
- Do not create `common`, `utils`, `misc`, `helpers`, or `god_engine` crates.
- Keep `lib.rs` as module declarations and intentional reexports.
- Keep public API minimal.
- Use typed errors in library crates.
- Keep unsafe code private and documented.
- Never serialize Rust native structs directly to disk or network.
- Add tests close to the behavior being changed.
