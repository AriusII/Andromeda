# P09 — Statistics, Procedure Store et optimizer V0

## But

Introduire des statistiques versionnées, un coût V0 explicable et un optimizer borné sans composants learned autoritaires.

## Pourquoi cette phase existe

andromeda-statistics et andromeda-optimizer existent comme propriétaires de contrats et DecisionTrace. Le corpus scientifique impose histogrammes, cost-based planning et prudence learned.

## Dépendances entrantes

- P03 catalog versions.
- P04 generic procedures.
- P05 storage metrics.
- P07 audit/observability.

## Objectifs

- Publier StatsVersion candidate -> validating -> published.
- Créer Procedure Store runtime records avec WalBytes, RowsAffected, TempBytes, ErrorKind.
- Créer coût V0 : Cpu + LogicalIO + PhysicalIO + Wal + Temp + Network + RiskPenalty.
- Créer PlanCacheKey strict.
- Créer DecisionTrace lisible.

## Tâches détaillées

- Définir StatsObject : rowcount, distinct, histogram, density, skew, sample, source snapshot.
- Implémenter stale-use policy et publication switch.
- Définir bounded candidate enumeration et plan rejection reasons.
- Créer PlanClass Generic/Small/Medium/Large/Skewed/StructuredObjectSmall/Large/Maintenance.
- Tester que ScenarioEvidence ne force jamais plan.
- Créer regression detection Procedure Store.

## Livrables attendus

- StatsObject v0
- StatsVersion publication v0
- Procedure Store v0
- PlanCacheKey v0
- DecisionTrace optimizer v0

## Personnes mobilisées

| Personne | Scope | Responsabilité |
| --- | --- | --- |
| Personne 03 | Lead Catalog, ContractHash et CatalogVersion | Structure le catalogue, les contrats, les versions, la compatibilité et les preuves de publication. |
| Personne 14 | Lead audit, observabilité et DecisionTrace | Garantit traces, audit ledger, corrélation, explanation post-mortem et métriques utiles. |
| Personne 15 | Lead optimizer, PlanCacheKey et plan classes | Structure coût V0, bounded candidates, PlanClass, hysteresis et DecisionTrace optimizer. |
| Personne 16 | Lead statistics, Procedure Store et feedback | Publie StatsVersion candidates, histogrammes, skew, NDV, Procedure Store et regression detection. |
| Personne 18 | Lead hardware CPU/NVMe performance | Pilote profils x64/arm64, SIMD runtime dispatch, NVMe queues, wear metrics et benchs mesurés. |

## Files d’attente de travail

| Queue | Contenu | Dépendance |
|---|---|---|
| Q1 — cadrage | Specs, ADR, invariants, critères de rejet. | Toujours en premier. |
| Q2 — preuve locale | Unit/property/golden tests du composant. | Q1 terminée. |
| Q3 — intégration | Liaison avec crates propriétaires et traces. | Q2 terminée. |
| Q4 — recovery/security | Crash/recovery ou security/admission si état durable ou externe. | Q3 terminée. |
| Q5 — documentation evidence | Rapport de validation et runbook si opérationnel. | Q4 terminée. |

## Tests et validations

- cargo test -p andromeda-statistics --test stats_publication_switch_tests -- --nocapture
- cargo test -p andromeda-optimizer --test srpl_optimizer_pipeline_contract -- --nocapture
- plan cache identity property tests

## Critères de sortie

- Un plan actif référence CatalogVersion + StatsVersion + PolicyVersion + ContractHash.
- Un candidat stats rejeté ne modifie jamais les plans.
- DecisionTrace explique candidats considérés, rejetés et coûts.
- Fallback classique existe sans learned component.

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
