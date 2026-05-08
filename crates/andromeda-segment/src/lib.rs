#![forbid(unsafe_code)]
#![doc = r#"
Future C5 owner scaffold for Andromeda durable storage segments.

This crate is intentionally behavior-free. It documents the boundary that may
eventually own segment descriptors, segment indexes, sealing rules, immutable
publication evidence, and recovery-facing segment metadata.

C5 invariants:

- Segment publication must be backed by durable WAL and manifest evidence.
- Segment metadata bytes must be versioned and explicitly encoded.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
- No behavior has moved into this crate in this scaffold.
"#]
