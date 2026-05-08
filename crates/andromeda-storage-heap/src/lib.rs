#![forbid(unsafe_code)]
#![doc = r#"
Future C5 owner scaffold for Andromeda heap storage.

This crate is intentionally behavior-free. It documents the boundary that may
eventually own heap page interpretation, slot directory policy, row placement,
scans, vacuum planning, and heap recovery evidence.

C5 invariants:

- Heap visibility must not outrun durable transaction commit evidence.
- Heap page flushes must not outrun durable WAL coverage through their page LSNs.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
- No behavior has moved into this crate in this scaffold.
"#]
