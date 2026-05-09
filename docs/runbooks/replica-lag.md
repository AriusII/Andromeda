# Replica Lag Runbook

## Purpose

Use this runbook when a replica falls behind the primary, RPO/RTO targets are at
risk, WAL retention grows, or promotion eligibility is being discussed.

## Prerequisites

- Current primary, membership epoch, quorum members, node roles, and RPO/RTO are
  known.
- Each replica reports durable LSN, applied LSN, safe LSN, lag bytes, lag time,
  apply rate, health, and divergence status.
- Fencing authority is available before any promotion decision.

## Procedure

1. Classify lag for each replica: lag bytes, lag time, apply rate, safe LSN,
   membership epoch, and health state.
2. Compare observed lag against RPO/RTO.
3. Hold WAL required for replica catch-up unless an approved snapshot resync
   decision replaces replay.
4. Confirm backup/PITR and forensic retention are not weakened to relieve lag.
5. Degrade read or promotion eligibility when lag violates policy.
6. Use snapshot resync when replay cannot catch up within policy or required WAL
   was reclaimed.
7. Do not promote a lagging, divergent, stale-epoch, or unhealthy replica.
8. Before any promotion, verify quorum, fencing, highest eligible durable LSN,
   membership epoch advancement, WAL chain compatibility, and recovery to a
   consistent LSN.
9. Clear degraded status only after safe LSN advances and evidence is retained.

## Validation Commands

```powershell
cargo test -p andromeda-storage --test hadr_membership_store_contract --locked -- --nocapture
cargo test -p andromeda-storage --test quorum_membership_contract --locked -- --nocapture
cargo test -p andromeda-storage --test hadr_promotion_runtime_contract --locked -- --nocapture
cargo test -p andromeda-storage --test wal_shipping_reclaimability_contract --locked -- --nocapture
```

## Escalate When

- WAL needed for catch-up was reclaimed.
- Lagging replica promotion is requested.
- Fencing cannot be proven.
- RPO/RTO is asserted without measured lag evidence.
