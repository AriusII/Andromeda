#![forbid(unsafe_code)]
#![doc = r#"
Future C5 owner scaffold for Andromeda durable page formats.

This crate is intentionally behavior-free. It documents the boundary that may
eventually own page headers, trailers, payload bounds, checksums, page LSN
policy, and explicit page codecs.

C5 invariants:

- A page flush must not outrun durable WAL coverage through the page LSN.
- Page bytes must be versioned and explicitly encoded.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
- No behavior has moved into this crate in this scaffold.
"#]
