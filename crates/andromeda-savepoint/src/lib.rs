#![forbid(unsafe_code)]
#![doc = r#"
Future C5 owner scaffold for Andromeda savepoint stacks and partial rollback metadata.

This crate is intentionally behavior-free. It documents the boundary that may
eventually own savepoint identifiers, nesting rules, bounded write-set evidence,
and rollback coordination.

C5 invariants:

- Savepoint rollback must not publish commit visibility before durable WAL evidence exists.
- Durable savepoint evidence must be bounded, versioned, and explicitly encoded.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
- No behavior has moved into this crate in this scaffold.
"#]
