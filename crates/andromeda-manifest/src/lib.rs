#![forbid(unsafe_code)]
#![doc = r#"
Future C5 owner scaffold for Andromeda manifests and root pointers.

This crate is intentionally behavior-free. It documents the boundary that may
eventually own manifest publication, root pointers, checkpoint evidence,
recovery floors, and storage topology descriptors.

C5 invariants:

- Manifest switches must prove durable WAL coverage for checkpoint and recovery-floor evidence.
- Manifest bytes must be versioned and explicitly encoded.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
- No behavior has moved into this crate in this scaffold.
"#]
