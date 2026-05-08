#![forbid(unsafe_code)]
#![doc = r#"
Future C5 owner scaffold for Andromeda operational runbook descriptors.

This crate is intentionally behavior-free. It documents the boundary that may
eventually own typed runbook descriptors for backup, restore, HA/DR, forensic
startup, and incident response procedures.

Current status: ownership-boundary scaffold only. It does not claim production
readiness, executable automation, or operational completeness.

C5 invariants:

- Runbooks must describe preconditions, evidence, operator actions, validation, and rollback limits.
- Administration and HA/DR controls must stay off the Application Surface.
- Runbook evidence must be durable and auditable; RAM and temporary state are not truth.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- No behavior has moved into this crate in this scaffold.
"#]
