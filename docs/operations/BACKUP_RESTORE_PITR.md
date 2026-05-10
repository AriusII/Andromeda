# Backup, restore, and PITR

> **Status:** Operational guidance  
> **Audience:** DBAs, SREs, maintainers, support engineers

## In this article

- Define the operational scope.
- State required policies.
- State required traces and validation.

## Purpose

Backup is not HA/DR. Backup protects against corruption, human error, destructive changes, ransomware, and silent compromise.

## Required policies

| Policy | Requirement |
|---|---|
| SecurityPolicy | Enforce identity, permissions, surface scope, and audit. |
| StoragePolicy | Define HotStore, ColdStore, compression, segments, and snapshots. |
| TransactionPolicy | Define isolation, timeout, retry, and durability mode. |
| ResourcePolicy | Bound CPU, RAM, NVMe, GPU, temp, spill, and ResultStream size. |
| RecoveryPolicy | Define snapshot, WAL retention, restore, PITR, and forensic behavior. |
| AuditPolicy | Define trace capture, retention, export, and tamper detection. |

## Required evidence

```text
TraceId
PolicyVersion
CatalogVersion when applicable
StatsVersion when applicable
PrincipalId when applicable
LastValidLsn when applicable
RecoveryReport when applicable
DecisionTrace when applicable
```

## Operational rule

> [!IMPORTANT]
> If an operation affects truth, security, recovery, or availability, it must be auditable and reproducible from recorded evidence.
