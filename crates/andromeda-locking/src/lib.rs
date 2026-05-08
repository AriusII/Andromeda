#![forbid(unsafe_code)]
#![doc = r#"
Future C5 owner scaffold for Andromeda lock coordination.

This crate is intentionally behavior-free. It documents the boundary that may
eventually own lock modes, ownership lifecycle, wait queues, deadlock evidence,
and cleanup policy.

C5 invariants:

- Lock release must not make a commit visible before its commit WAL record is durable.
- Lock cleanup must preserve recovery-consistent transaction state.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
- No behavior has moved into this crate in this scaffold.
"#]
