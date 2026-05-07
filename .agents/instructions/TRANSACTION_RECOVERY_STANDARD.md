# Transaction and Recovery Standard

## Requirements

- Commit visible equals WAL durable.
- RAM is never truth.
- The canonical state is the last valid cold snapshot plus durable WAL.
- Recovery must define REDO, incomplete transaction handling, manifest validation, and corruption response.
- Crash tests are mandatory for transaction semantics.

## HA/DR and Quorum (DEC-020)

- **Quorum Protocol:** Majority quorum is required for leader promotion and synchronous replication durability in V0.
- **Membership Epoch:** Every membership change increments a global `MembershipEpoch` to prevent stale leaders or split-brain scenarios.
- **LSN Ranking:** Leader election is decided by LSN-based ranking; nodes with higher durable LSNs are prioritized.
