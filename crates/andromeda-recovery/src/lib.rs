#![forbid(unsafe_code)]
#![doc = r#"
Future C5 owner scaffold for Andromeda startup recovery and replay planning.

This crate is intentionally behavior-free. It documents the boundary that may
eventually own durable artifact discovery, replay selection, recovery floors,
startup modes, recovery reports, and forensic evidence.

C5 invariants:

- Recovery truth must come from durable WAL and durable artifacts, not RAM summaries.
- Visible commits after restart must be reconstructed only from durable commit evidence.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
- No behavior has moved into this crate in this scaffold.
"#]
