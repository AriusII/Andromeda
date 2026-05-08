#![forbid(unsafe_code)]
#![doc = r#"
Future C5 owner scaffold for Andromeda MVCC visibility.

This crate is intentionally behavior-free. It documents the boundary that may
eventually own snapshot visibility, version lifecycle policy, active-reader
tracking, and version garbage-collection eligibility.

C5 invariants:

- A committed version must not become visible before its commit WAL record is durable.
- MVCC visibility must be derived from durable transaction evidence, not RAM alone.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
- No behavior has moved into this crate in this scaffold.
"#]
