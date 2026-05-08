#![forbid(unsafe_code)]
#![doc = r#"
Future C5 owner scaffold for Andromeda high availability and disaster recovery.

This crate is intentionally behavior-free. It documents the boundary that may
eventually own replication roles, quorum evidence, fencing, promotion epochs,
WAL shipping state, and cluster recovery reports.

Current status: ownership-boundary scaffold only. It does not claim production
readiness, failover safety, or operational completeness.

C5 invariants:

- Promotion and failover must be backed by quorum, fencing, epoch, and durable WAL evidence.
- Replicas must not publish visible commits without durable commit evidence.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
- No behavior has moved into this crate in this scaffold.
"#]
