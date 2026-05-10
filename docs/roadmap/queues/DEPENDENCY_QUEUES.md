# Files d’attente pyramidales

## Idée

On ne lance pas 20 personnes en parallèle sur les mêmes frontières. On lance des files dépendantes, où certaines personnes libèrent le terrain pour d’autres.

## Niveau 1 — Socle de preuve

| Personnes | Travail | Débloque |
|---|---|---|
| 1, 2 | Current state, CI, ADR, crate topology, validation gates. | Tout le monde. |
| 3, 4 | CatalogObjectModel, ContractHash, DefinitionBatch. | 5, 6, 7, 15, 16. |
| 8, 9, 10 | WAL, storage, recovery specs et preuves. | 7, 11, 13, 14, 20. |
| 12, 13, 14 | RPC/security/audit admission evidence. | P08, P15. |

## Niveau 2 — Vertical durable

| Personnes | Travail | Débloque |
|---|---|---|
| 7 | Procedure runtime et ResultStream. | Exécution générique P04. |
| 8 | WAL durable et fences. | Storage/recovery/transaction. |
| 9 | Heap/page/manifests. | Recovery, Maps, backup. |
| 10 | Recovery reports et crash runner. | Release evidence. |
| 14 | Traces et audit correlation. | Debug, forensic, release. |

## Niveau 3 — Généralisation

| Personnes | Travail | Débloque |
|---|---|---|
| 5, 6 | SRPL parser/binder/IR/cardinality. | Generic Procedure + optimizer. |
| 3, 4 | Catalog durable et DefinitionBatch. | Generic Procedure + stats. |
| 11 | MVCC/isolation/locking. | Secure runtime et production-like tests. |
| 12, 13 | QUIC + IAM admission. | Runtime réseau. |

## Niveau 4 — Adaptatif et performance

| Personnes | Travail | Condition |
|---|---|---|
| 15, 16 | Optimizer, PlanCacheKey, StatsVersion, Procedure Store. | Truth path stable. |
| 17 | Maps/analytics CPU. | Storage + stats stable. |
| 18 | CPU/NVMe performance. | Storage + metrics stables. |
| 19 | GPU optionnel. | CPU analytics + fallback stable. |

## Niveau 5 — Opérations Enterprise

| Personnes | Travail | Condition |
|---|---|---|
| 20, 10, 8, 9 | Backup, restore, PITR, forensic. | WAL/storage/recovery stables. |
| 20, 12, 13, 14 | HA/DR, quorum, fencing, cluster audit. | Backup/PITR et RPC/security stables. |
| 1, 2, 14, 20 | Release evidence et runbooks. | Toutes phases exit. |

## Règle de blocage

Si une personne aval dépend d’une preuve absente, elle doit écrire un stub de contrat ou un test de rejet, pas simuler la vérité dans son propre crate.
