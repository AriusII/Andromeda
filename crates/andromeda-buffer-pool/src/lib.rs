#![forbid(unsafe_code)]
#![doc = r#"
Future C5 owner scaffold for Andromeda buffer-pool coordination.

This crate is intentionally behavior-free. It documents the boundary that may
eventually own buffer frames, pinning, dirty tracking, eviction, flush
scheduling, and WAL durability fences.

C5 invariants:

- Dirty page flush must not outrun durable WAL coverage through the page LSN.
- In-memory residency is advisory and must not be treated as durable truth.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
- No behavior has moved into this crate in this scaffold.
"#]
