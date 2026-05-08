#![forbid(unsafe_code)]
#![doc = r#"
Future C5 owner scaffold for logical transaction terminal evidence.

This crate is intentionally behavior-free. It documents the boundary that may
eventually own transaction-log record shapes and replay-facing transaction
classification after an explicit split from existing transaction, WAL, and
storage crates.

C5 invariants:

- A commit must not become visible before its commit WAL record is durable.
- Terminal transaction evidence must be read from a verified durable WAL prefix.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
- No behavior has moved into this crate in this scaffold.
"#]
