#![forbid(unsafe_code)]
#![doc = r#"
Future C5 owner scaffold for Andromeda restore orchestration.

This crate is intentionally behavior-free. It documents the boundary that may
eventually own restore plans, point-in-time recovery targets, artifact
verification, WAL archive ranges, and restore validation reports.

C5 invariants:

- Restored visible commits must be reconstructed only from durable commit evidence.
- Restore success must be backed by durable backup artifacts and WAL archive coverage.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
- No behavior has moved into this crate in this scaffold.
"#]
