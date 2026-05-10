# P02 — Vertical durable Inventory.ProductStock

## But

Prouver le chemin Procedure -> Admission -> Transaction -> WAL -> Heap/Page -> Recovery -> ResultStream sur un cas métier minimal.

## Pourquoi cette phase existe

Le dépôt possède déjà un V0 Inventory.ReserveStock avec ProductStock, HREDOV1, FileWal, ResultStream metadata et tests de rejet pré-transaction. Il faut faire de ce vertical un socle durable stable, pas seulement un prototype local.

## Dépendances entrantes

- P01 specs WAL, ProcedureContract, RPC, Recovery, Page/Heap.
- Crates andromeda-inventory-demo, andromeda-exec, andromeda-wal, andromeda-storage-heap, andromeda-recovery disponibles.

## Objectifs

- Faire du ProductStock path le témoin de vérité durable.
- Prouver que ProductStock ne publie jamais sans TxCommit durable.
- Prouver que les rejets contract/security/protocol ne touchent ni WAL ni heap.
- Prouver que ResultStream expose metadata puis batch puis completion.

## Tâches détaillées

- Consolider Inventory.ReserveStock comme fixture canonique.
- Stabiliser ProductStockRow, HeapRowRedoPayloadV1, PageId, PageSize KiB16 et PageLSN.
- Connecter durable_commit_lsn, redo_record_lsn et product_stock_commit dans un même evidence model.
- Renforcer les tests : malformed frame, invalid payload domain, transaction-bearing client frame, contract mismatch, missing permission.
- Renforcer recovery : replay durable FileWal, only committed redo, RecoveryReport, reconstructed page rows.
- Créer un test de crash runner réel après WAL append et avant ack client.

## Livrables attendus

- ProductStock durable path acceptance report
- Recovery evidence report for FileWal
- ResultStream ordering contract report
- Pre-transaction rejection evidence matrix

## Personnes mobilisées

| Personne | Scope | Responsabilité |
| --- | --- | --- |
| Personne 02 | Lead workspace Rust, topologie crates et CI | Garde Cargo, lints, dependency topology, cargo check/test/clippy/fmt et politiques supply-chain sous contrôle. |
| Personne 07 | Lead admission et Procedure execution runtime | Assure la chaîne Procedure -> contract binding -> permission -> transaction -> completion. |
| Personne 08 | Lead WAL, FileWal, LSN et durabilité | Garantit FileWal, WAL codecs, durable prefix, flush_through, transaction classification et fences. |
| Personne 09 | Lead storage pages, heap, BufferPool et manifests | Conçoit page/heap/index/store, PageLSN, manifest, SegmentIndex, snapshot froid et hot/cold storage. |
| Personne 10 | Lead recovery, crash runner et forensic startup | Prouve REDO/UNDO, RecoveryReport, crash matrix, replay WAL et modes Online/ReadOnly/ForensicOnly. |
| Personne 14 | Lead audit, observabilité et DecisionTrace | Garantit traces, audit ledger, corrélation, explanation post-mortem et métriques utiles. |

## Files d’attente de travail

| Queue | Contenu | Dépendance |
|---|---|---|
| Q1 — cadrage | Specs, ADR, invariants, critères de rejet. | Toujours en premier. |
| Q2 — preuve locale | Unit/property/golden tests du composant. | Q1 terminée. |
| Q3 — intégration | Liaison avec crates propriétaires et traces. | Q2 terminée. |
| Q4 — recovery/security | Crash/recovery ou security/admission si état durable ou externe. | Q3 terminée. |
| Q5 — documentation evidence | Rapport de validation et runbook si opérationnel. | Q4 terminée. |

## Tests et validations

- cargo test -p andromeda-inventory-demo --test v0_vertical_e2e -- --nocapture
- cargo test -p andromeda-wal --test file_wal_contract -- --nocapture
- cargo test -p andromeda-recovery --test file_wal_recovery_contract -- --nocapture

## Critères de sortie

- Crash avant durable commit : stock inchangé.
- Crash après durable commit : stock reconstruit.
- Frame ou contract invalides : aucun WAL append.
- Completion ResultStream porte durable_lsn non nul.

## Risques principaux

| Risque | Réponse déterministe |
|---|---|
| Le scope dérive vers une fonctionnalité attractive mais non dépendante. | La fonctionnalité est repoussée en phase ultérieure ou sandbox C0/C1. |
| Une décision n’a pas de trace ou pas de version. | Rejet de la décision jusqu’à ajout de PolicyVersion, CatalogVersion, StatsVersion ou evidence appropriée. |
| Un test passe sans prouver l’invariant métier ou durable. | Ajouter golden/property/crash test ciblé ; ne pas compter le test comme exit evidence. |
| Un composant adaptive devient autoritaire. | Restaurer fallback classique et DecisionTrace ; déplacer en evidence non décisionnaire. |

## Anti-patterns spécifiques

- Ajouter une surface SQL ad hoc.
- Publier un état visible sans preuve durable.
- Cacher une décision critique dans un helper non observé.
- Traiter une fixture, un benchmark, une trace ou un résultat GPU comme vérité système.
- Déplacer une responsabilité vers un crate de convenance plutôt que vers son owner.

## Sortie opérationnelle attendue

À la fin de cette phase, l’équipe doit pouvoir montrer une preuve reproductible : code propriétaire, tests ciblés, trace ou rapport, et comportement de recovery ou fail-closed si la phase touche durable state ou surface externe.
