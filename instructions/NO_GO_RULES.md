# No-Go Rules

## Immediate rejection

Reject or escalate any proposal that introduces:

- Application-facing ad hoc SQL.
- gRPC as the RPC surface.
- JSON as a default runtime wire format.
- GPU participation in commit, WAL, rollback, or recovery.
- Unrecoverable state changes.
- Hidden mutable global state in critical paths.
- Unbounded loops in SRPL core semantics.
- Native network or filesystem access from a Procedure.
- Silent type conversions in contracts.
- Security bypass through certificates.
