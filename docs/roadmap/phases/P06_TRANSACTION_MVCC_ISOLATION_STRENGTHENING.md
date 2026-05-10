# P06 — Transaction, MVCC, isolation et rollback

## But

Durcir la machine d’état transactionnelle, la visibilité MVCC, les conflits et les garanties d’isolation.

## Pourquoi cette phase existe

Les documents rappellent que ACID ne suffit pas : il faut parler anomalies interdites. Le dépôt possède transaction, transaction-log, mvcc, locking, savepoint. Cette phase clarifie la vérité concurrente.

## Dépendances entrantes

- P05 storage durable.
- P04 generic Procedure runtime.

## Objectifs

- Formaliser Created/Active/Committing/Committed/Failed/Poisoned/RollingBack/RolledBack/Disposed.
- Définir isolation policies V0 par anomalies interdites.
- Stabiliser rollback evidence et savepoint behavior.
- Créer ActiveSnapshotRegistry et MVCC GC pins.

## Tâches détaillées

- Écrire tests de transitions transactionnelles valides/interdites.
- Connecter transaction manager au heap/page runtime.
- Définir write conflict policy et retryability.
- Modéliser long readers et OldestActiveSnapshotTs.
- Prouver rollback table/index/map state.
- Émettre TransactionTrace avec TxId, durable LSN, state transition, reason code.

## Livrables attendus

- TransactionStateMachine v0 implementation evidence
- MVCC visibility v0
- RollbackEvidence v0
- IsolationPolicy matrix

## Personnes mobilisées

| Personne | Scope | Responsabilité |
| --- | --- | --- |
| Personne 07 | Lead admission et Procedure execution runtime | Assure la chaîne Procedure -> contract binding -> permission -> transaction -> completion. |
| Personne 08 | Lead WAL, FileWal, LSN et durabilité | Garantit FileWal, WAL codecs, durable prefix, flush_through, transaction classification et fences. |
| Personne 09 | Lead storage pages, heap, BufferPool et manifests | Conçoit page/heap/index/store, PageLSN, manifest, SegmentIndex, snapshot froid et hot/cold storage. |
| Personne 10 | Lead recovery, crash runner et forensic startup | Prouve REDO/UNDO, RecoveryReport, crash matrix, replay WAL et modes Online/ReadOnly/ForensicOnly. |
| Personne 11 | Lead transaction, MVCC, isolation et locking | Durcit TransactionStateMachine, MVCC visibility, rollback, savepoints, locks et anomalies interdites. |
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

- cargo test -p andromeda-transaction --all-targets
- cargo test -p andromeda-mvcc --all-targets
- cargo test -p andromeda-locking --all-targets
- property tests sur state transitions

## Critères de sortie

- Poisoned interdit toute continuation normale.
- Rollback restaure ou annule les intents visibles.
- Les lecteurs snapshot voient un état cohérent.
- Les anomalies permises/interdites sont documentées par policy.

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
