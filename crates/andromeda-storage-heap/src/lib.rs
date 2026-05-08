#![forbid(unsafe_code)]
#![doc = r#"
C5 owner crate for Andromeda heap storage boundaries.

This crate starts the extraction of heap-owned durable slot metadata from
`andromeda-storage`. Full heap page mutation, scans, vacuum planning, and redo
remain in `andromeda-storage` until their WAL and recovery evidence contracts
are promoted together.

C5 invariants:

- Heap visibility must not outrun durable transaction commit evidence.
- Heap page flushes must not outrun durable WAL coverage through their page LSNs.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
"#]

mod slot;

pub use slot::SlotEntry;
