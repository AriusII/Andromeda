# Replica lag runbook

> **Status:** Operational runbook  
> **Audience:** DBAs, SREs, maintainers, support engineers  
> **Scope:** Andromeda operations

## In this article

- Identify the trigger.
- Execute immediate containment.
- Diagnose the affected subsystem.
- Validate recovery or mitigation.
- Preserve audit and forensic evidence.

## Trigger

Replica LastAppliedLsn is behind required promotion or retention window.

## Impact

This condition can affect availability, recovery confidence, performance stability, or security evidence. Treat it as operationally significant until validation proves otherwise.

## Immediate containment

- Measure LastDurableLsn and LastAppliedLsn.
- Protect WAL retention.
- Prevent promotion if lag violates policy.
- Resync from snapshot if needed.
- Emit ClusterEventTrace.


## Diagnosis checklist

| Check | Evidence |
|---|---|
| Current database state | Online, ReadOnly, Suspended, Recovering, or ForensicOnly. |
| Recent Procedure invocations | ProcedureInvocationTrace and Procedure Store entries. |
| WAL state | Last durable LSN, queue depth, flush latency, truncation status. |
| Catalog state | CatalogVersion, last DefinitionBatch, compatibility changes. |
| Security state | Principal, certificate, permissions, policy version. |
| Storage state | Manifest, SegmentIndex, page/hash validation, HotStore/ColdStore health. |
| Resource state | CPU, RAM, NVMe, GPU, temp, spill, network backpressure. |

## Validation

A mitigation is valid only when:

```text
critical traces are emitted
state transition is understood
no hidden mutation remains
recovery or rollback behavior is known
operator evidence is preserved
```

## Escalation

Escalate when:

- the database cannot open Online or ReadOnly;
- the WAL chain is corrupted in the middle;
- the active manifest and previous manifest are both invalid;
- a security principal or certificate may be compromised;
- promotion or restore evidence is incomplete.

## Post-incident evidence

Record:

```text
IncidentId
Start mode
Affected database
Affected Procedure or object
Principal and certificate if applicable
Last valid LSN
Manifest version
PolicyVersion
RecoveryReport or AdminOperationTrace
Final open mode
```
