#![forbid(unsafe_code)]
#![doc = r#"
Future C5 owner scaffold for Andromeda backup planning and evidence.

This crate is intentionally behavior-free. It documents the boundary that may
eventually own backup plans, backup artifacts, checkpoint evidence, WAL archive
requirements, retention policy, and validation reports.

Current status: ownership-boundary scaffold only. It does not claim production
readiness, recoverability, or operational completeness.

C5 invariants:

- Backup consistency must be backed by durable checkpoint and WAL archive evidence.
- Backup metadata bytes must be versioned and explicitly encoded.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
- No behavior has moved into this crate in this scaffold.
"#]
