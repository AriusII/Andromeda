# P11 — Performance CPU/NVMe et gouvernance ressources

## But

Optimiser seulement après preuve fonctionnelle, avec métriques, profils hardware, fallback et policies.

## Pourquoi cette phase existe

Le projet cible x64/arm64, NVMe hot path et HDD cold truth. L’accélération CPU et l’ordonnancement NVMe doivent être mesurés, pas devinés.

## Dépendances entrantes

- P05 storage stable.
- P09 stats/optimizer metrics.
- P10 analytics CPU.

## Objectifs

- Définir HardwareProfile runtime.
- Utiliser SIMD seulement via runtime detection et fallback scalar.
- Mesurer WAL flush latency, queue depth, wear, write amplification.
- Créer ResourceBudget par Procedure et job.

## Tâches détaillées

- Créer CPU kernel registry : crc/hash/compression/scan scalar + accelerated.
- Définir x64-baseline/x64-avx2/x64-avx512 et arm64-neon/sve2 profiles.
- Créer NVMe P0 WAL queue séparée des temp/spill.
- Mesurer BufferPool hit ratio, dirty ratio, stalls.
- Benchmarks sous profil matériel fixé.
- Interdire target-cpu=native global pour binaries distribués.

## Livrables attendus

- HardwareProfile v0
- CpuKernelDispatch v0
- ResourceBudget v0
- NVMe Scheduler metrics v0
- Performance evidence registry

## Personnes mobilisées

| Personne | Scope | Responsabilité |
| --- | --- | --- |
| Personne 02 | Lead workspace Rust, topologie crates et CI | Garde Cargo, lints, dependency topology, cargo check/test/clippy/fmt et politiques supply-chain sous contrôle. |
| Personne 09 | Lead storage pages, heap, BufferPool et manifests | Conçoit page/heap/index/store, PageLSN, manifest, SegmentIndex, snapshot froid et hot/cold storage. |
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

- same vectors scalar/SIMD
- feature-disabled path tests
- cargo bench selected WAL/storage/scan benches
- ResourceBudget property tests

## Critères de sortie

- Tout kernel accéléré a fallback CPU et tests équivalents.
- P0 WAL flush ne partage pas la même file logique que temp/spill.
- Les optimisations acceptées ont DecisionTrace ou BenchmarkEvidence.
- Aucune accélération ne modifie la sémantique durable.

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
