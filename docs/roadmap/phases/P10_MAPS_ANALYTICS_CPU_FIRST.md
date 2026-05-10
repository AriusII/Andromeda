# P10 — Maps, analytique CPU et columnar avant GPU

## But

Construire les Maps matérialisées, le grain analytique et les layouts columnar CPU avant toute accélération GPU.

## Pourquoi cette phase existe

Les docs définissent Map comme projection matérialisée, non View virtuelle. andromeda-maps possède MapDescriptor et publication evidence mais ne matérialise pas encore. Il faut d’abord le CPU correct.

## Dépendances entrantes

- P03 catalog.
- P05 storage/SegmentIndex.
- P09 stats.

## Objectifs

- Déclarer grain et summarizability.
- Créer MapDescriptor avec refresh mode, staleness, publication evidence.
- Implémenter Immediate borné, Incremental delta, Deferred, SnapshotOnly.
- Créer columnar segments et pruning metadata.

## Tâches détaillées

- Écrire MapConsistencyPolicy avec coût maximum et staleness.
- Créer tests Immediate Map simple dans transaction avec WAL records table + map.
- Créer MapDeltaLog incremental contrôlé.
- Créer Map rebuild/recovery evidence.
- Définir column chunks, min/max, optional bloom, source snapshot binding.
- Empêcher Map refresh de dépasser WAL priority.

## Livrables attendus

- MapDescriptor v0
- MapPublicationEvidence v0
- Immediate Map v0
- Incremental Map delta v0
- Columnar Map segment v0

## Personnes mobilisées

| Personne | Scope | Responsabilité |
| --- | --- | --- |
| Personne 09 | Lead storage pages, heap, BufferPool et manifests | Conçoit page/heap/index/store, PageLSN, manifest, SegmentIndex, snapshot froid et hot/cold storage. |
| Personne 14 | Lead audit, observabilité et DecisionTrace | Garantit traces, audit ledger, corrélation, explanation post-mortem et métriques utiles. |
| Personne 15 | Lead optimizer, PlanCacheKey et plan classes | Structure coût V0, bounded candidates, PlanClass, hysteresis et DecisionTrace optimizer. |
| Personne 16 | Lead statistics, Procedure Store et feedback | Publie StatsVersion candidates, histogrammes, skew, NDV, Procedure Store et regression detection. |
| Personne 17 | Lead Maps, analytics CPU et columnar | Déclare grain, summarizability, Map refresh modes, column chunks et analytics CPU avant GPU. |

## Files d’attente de travail

| Queue | Contenu | Dépendance |
|---|---|---|
| Q1 — cadrage | Specs, ADR, invariants, critères de rejet. | Toujours en premier. |
| Q2 — preuve locale | Unit/property/golden tests du composant. | Q1 terminée. |
| Q3 — intégration | Liaison avec crates propriétaires et traces. | Q2 terminée. |
| Q4 — recovery/security | Crash/recovery ou security/admission si état durable ou externe. | Q3 terminée. |
| Q5 — documentation evidence | Rapport de validation et runbook si opérationnel. | Q4 terminée. |

## Tests et validations

- cargo test -p andromeda-maps --all-targets
- Map publication contract tests
- Map recovery/rebuild tests
- summarizability checks

## Critères de sortie

- Une Map n’est jamais source truth.
- Immediate Map coût borné ou rejeté.
- SnapshotOnly Map cohérente avec snapshot publié.
- Map maintenance ne starve jamais WAL flush.

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
