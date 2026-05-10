# Allocation des 20 personnes

## Principe

Les 20 personnes sont supposées hautement qualifiées et capables de lire les ADR. La distribution ci-dessous évite de créer 20 flux concurrents qui se marchent dessus. La méthode est pyramidale : fondations et preuves d’abord, accélération et opérations ensuite.

## Matrice des scopes

| Personne | Scope principal | Responsabilité |
| --- | --- | --- |
| Personne 01 | Lead doctrine, ADR et cohérence documentaire | Garantit que la roadmap, les ADR, les specs et les règles de rejet restent alignées avec la doctrine Andromeda. |
| Personne 02 | Lead workspace Rust, topologie crates et CI | Garde Cargo, lints, dependency topology, cargo check/test/clippy/fmt et politiques supply-chain sous contrôle. |
| Personne 03 | Lead Catalog, ContractHash et CatalogVersion | Structure le catalogue, les contrats, les versions, la compatibilité et les preuves de publication. |
| Personne 04 | Lead DefinitionBatch et migration contrôlée | Pilote DryRun, graphe de dépendances, rollback de batch, mutation catalog durable et audit. |
| Personne 05 | Lead SRPL parser, AST et diagnostics | Rend la syntaxe SRPL strictement bornée, stable, diagnostiquable et sans SQL dynamique. |
| Personne 06 | Lead SRPL binder, IR et cardinalité | Résout noms, types, cardinalités, absence explicite, read/write sets et Semantic IR. |
| Personne 07 | Lead admission et Procedure execution runtime | Assure la chaîne Procedure -> contract binding -> permission -> transaction -> completion. |
| Personne 08 | Lead WAL, FileWal, LSN et durabilité | Garantit FileWal, WAL codecs, durable prefix, flush_through, transaction classification et fences. |
| Personne 09 | Lead storage pages, heap, BufferPool et manifests | Conçoit page/heap/index/store, PageLSN, manifest, SegmentIndex, snapshot froid et hot/cold storage. |
| Personne 10 | Lead recovery, crash runner et forensic startup | Prouve REDO/UNDO, RecoveryReport, crash matrix, replay WAL et modes Online/ReadOnly/ForensicOnly. |
| Personne 11 | Lead transaction, MVCC, isolation et locking | Durcit TransactionStateMachine, MVCC visibility, rollback, savepoints, locks et anomalies interdites. |
| Personne 12 | Lead QUIC/RPC/protocol/runtime | Sépare transport QUIC et sémantique RPC, frame sequencing, ResultStream et runtime Quinn. |
| Personne 13 | Lead IAM, principal, security admission et permissions | Gère CertificateIdentity, UserPrincipal, permissions, policies, deny/allow et admission fail-closed. |
| Personne 14 | Lead audit, observabilité et DecisionTrace | Garantit traces, audit ledger, corrélation, explanation post-mortem et métriques utiles. |
| Personne 15 | Lead optimizer, PlanCacheKey et plan classes | Structure coût V0, bounded candidates, PlanClass, hysteresis et DecisionTrace optimizer. |
| Personne 16 | Lead statistics, Procedure Store et feedback | Publie StatsVersion candidates, histogrammes, skew, NDV, Procedure Store et regression detection. |
| Personne 17 | Lead Maps, analytics CPU et columnar | Déclare grain, summarizability, Map refresh modes, column chunks et analytics CPU avant GPU. |
| Personne 18 | Lead hardware CPU/NVMe performance | Pilote profils x64/arm64, SIMD runtime dispatch, NVMe queues, wear metrics et benchs mesurés. |
| Personne 19 | Lead GPU batch optionnel | Isole GPU_STATS/GPU_ANALYTICS/GPU_BENCHMARK avec fallback CPU et kill switches hors C5. |
| Personne 20 | Lead backup, restore, HA/DR et release operations | Pilote PITR, backup validation, WAL archives, quorum/fencing, runbooks, release gates et drills. |

## Règle de coordination

- Une personne ne publie pas de changement qui viole une frontière owner crate.
- Une personne C5 ne dépend pas d’un composant adaptive non validé.
- Une personne adaptive doit fournir fallback, metrics, DecisionTrace et disablement.
- Une personne operation/release exige evidence retenue, pas seulement existence de code.
