# storage-page-layout Checklist

Use this checklist when the skill is active.

## Required checks

- Confirm scope.
- Confirm project invariant alignment.
- Confirm versioning.
- Confirm observability.
- Confirm failure behavior.
- Confirm testing approach.
- Confirm rollback or disablement path.

## Andromeda-specific checks

- QUIC remains the transport.
- Protobuf does not imply gRPC.
- Procedures remain the only execution surface.
- WAL durability precedes visibility for persistent mutation.
- Security and audit requirements are explicit.
