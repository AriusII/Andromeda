# Four-engine model

> **Status:** Implementation architecture  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Group the detailed architecture into four implementation clusters.
- Reduce integration drift without creating a God Engine.
- Map clusters to current and future Rust crates.

## Why four clusters

The detailed architecture contains many engines and planes. For implementation governance, four clusters give a stable mental model while preserving module boundaries.

## Engine clusters

| Cluster | Purpose | Criticality posture |
|---|---|---|
| Contract and Language Engine | Catalog, Type System, Procedure contracts, SRPL, StructuredObjects, Maps definitions. | Strict, versioned, compile-time oriented. |
| Transactional Truth Engine | Transaction Kernel, WAL, MVCC, Storage, BufferPool, HotStore, ColdStore, manifests, recovery. | C5 for commit truth and recovery. |
| Surface and Governance Engine | QUIC/RPC, protocol frames, IAM, security, audit, admin, HA/DR, backup, PITR, forensic. | Fail-closed, audited, surface-separated. |
| Adaptive Intelligence and Hardware Engine | Procedure Store, optimizer, statistics, ScenarioEvidence, analytics, CPU/GPU/NVMe/HDD policies. | Advisory, bounded, explainable, disableable. |

## Contract and Language Engine

Owns:

```text
Catalog descriptors
Type descriptors
ProcedureContract
ContractHash
SRPL parser and binder
Semantic IR
StructuredObject contracts
Map descriptors
DefinitionBatch
Compatibility policy
```

Must not own:

```text
WAL flush
QUIC stream lifecycle
GPU runtime
storage file mutation
security bypass
```

## Transactional Truth Engine

Owns:

```text
Transaction states
MVCC visibility
WAL records
Commit durability
Rollback
BufferPool
Page format
HotStore and ColdStore
Manifest and SegmentIndex
RecoveryReport
```

Must not own:

```text
Procedure business meaning
IAM policy meaning
Plan choice
GPU acceleration
external client protocol negotiation
```

## Surface and Governance Engine

Owns:

```text
Application Surface
Administration Surface
HA/DR Cluster Surface
RPC frames
mTLS identity
CertificateIdentity
UserPrincipal
permissions and policies
audit ledger
backup and restore operations
cluster quorum and fencing
```

Must not own:

```text
storage truth decisions
physical plan choice
SRPL semantics
catalog publication without transaction
```

## Adaptive Intelligence and Hardware Engine

Owns:

```text
Procedure Store
StatsObject
StatsVersion
PlanCacheKey
PlanClass
Cost model
ScenarioEvidence
HardwareProfile
SIMD kernels
GPU batch jobs
analytics Maps
```

Must not own:

```text
commit truth
security admission
catalog publication
recovery truth
```

## Crate mapping rule

A crate should map to a real responsibility pressure, not to every small object. Do not split crates prematurely if integration evidence is still weak.
