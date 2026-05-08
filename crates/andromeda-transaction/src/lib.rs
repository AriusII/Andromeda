#![forbid(unsafe_code)]
#![doc = r#"
Future C5 owner scaffold for Andromeda transaction lifecycle coordination.

This crate is intentionally behavior-free. It documents the boundary that may
eventually own transaction lifecycle state after an explicit split from
`andromeda-tx`.

C5 invariants:

- A commit must not become visible before its commit WAL record is durable.
- Rollback paths that claim crash-recoverable terminal state require durable evidence.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
- No behavior has moved into this crate in this scaffold.
"#]
