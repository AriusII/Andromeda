# Mission-Critical Change Policy

## Purpose

Define additional constraints for critical Andromeda changes.

## C4/C5 subsystems

- WAL
- Recovery
- Storage format
- Transaction state machine
- Catalog publication
- QUIC/RPC contract validation
- IAM/security
- HA/DR quorum and fencing
- Backup/restore/PITR

## Required evidence

A change in these areas must include:

- invariant statement,
- failure mode analysis,
- tests,
- observability,
- rollback or containment,
- release gate status.

## Default decision

If evidence is missing, keep the change experimental or reject it.
