#![forbid(unsafe_code)]
#![doc = r#"
Future C5 owner scaffold for Andromeda disk-backed page storage.

This crate is intentionally behavior-free. It documents the boundary that may
eventually own page read, write, allocation, sync, and device error
classification.

C5 invariants:

- Durable page writes must not outrun WAL-before-page-flush policy.
- Page bytes must come from explicit, versioned codecs.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
- No behavior has moved into this crate in this scaffold.
"#]
