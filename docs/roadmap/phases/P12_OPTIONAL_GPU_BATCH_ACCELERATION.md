# P12 — GPU batch optionnel

## But

Ajouter GPU_STATS/GPU_ANALYTICS/GPU_BENCHMARK seulement comme accélération batch optionnelle, jamais C5.

## Pourquoi cette phase existe

Les documents GPU recommandent le GPU pour histogrammes, cardinalité, scans analytiques et benchmark scenarios, avec FP64/mémoire comme critères. Les docs projet interdisent GPU commit/recovery/security.

## Dépendances entrantes

- P10 Maps/analytics CPU.
- P11 Hardware/resource governance.
- P09 Stats publication.

## Objectifs

- Créer crate GPU optionnelle hors dépendances C5.
- Maintenir CPU fallback obligatoire.
- Valider GPU-produced stats par CPU avant publication.
- Créer kill switch et cancellation safe.
- Enregistrer GpuExecutionTrace.

## Tâches détaillées

- Définir GpuExecutionPolicy et GpuJobClass.
- Créer import scan qui interdit gpu crate dans WAL/recovery/transaction/security critical paths.
- Prototype GPU_STATS histogram sur Map SnapshotOnly ou stats candidate.
- Prototype GPU_ANALYTICS scan columnar non OLTP.
- Créer fallback reasons et validation state.
- Créer budget GPU memory/time/transfer bytes.

## Livrables attendus

- andromeda-gpu optional scaffold
- GpuExecutionPolicy v0
- GPU_STATS prototype
- GPU_ANALYTICS prototype
- GPU boundary tests

## Personnes mobilisées

| Personne | Scope | Responsabilité |
| --- | --- | --- |
| Personne 02 | Lead workspace Rust, topologie crates et CI | Garde Cargo, lints, dependency topology, cargo check/test/clippy/fmt et politiques supply-chain sous contrôle. |
| Personne 14 | Lead audit, observabilité et DecisionTrace | Garantit traces, audit ledger, corrélation, explanation post-mortem et métriques utiles. |
| Personne 16 | Lead statistics, Procedure Store et feedback | Publie StatsVersion candidates, histogrammes, skew, NDV, Procedure Store et regression detection. |
| Personne 17 | Lead Maps, analytics CPU et columnar | Déclare grain, summarizability, Map refresh modes, column chunks et analytics CPU avant GPU. |
| Personne 18 | Lead hardware CPU/NVMe performance | Pilote profils x64/arm64, SIMD runtime dispatch, NVMe queues, wear metrics et benchs mesurés. |
| Personne 19 | Lead GPU batch optionnel | Isole GPU_STATS/GPU_ANALYTICS/GPU_BENCHMARK avec fallback CPU et kill switches hors C5. |

## Files d’attente de travail

| Queue | Contenu | Dépendance |
|---|---|---|
| Q1 — cadrage | Specs, ADR, invariants, critères de rejet. | Toujours en premier. |
| Q2 — preuve locale | Unit/property/golden tests du composant. | Q1 terminée. |
| Q3 — intégration | Liaison avec crates propriétaires et traces. | Q2 terminée. |
| Q4 — recovery/security | Crash/recovery ou security/admission si état durable ou externe. | Q3 terminée. |
| Q5 — documentation evidence | Rapport de validation et runbook si opérationnel. | Q4 terminée. |

## Tests et validations

- GPU job cancellation does not affect transaction state
- CPU fallback always available
- GPU-produced stats rejected until CPU validation
- dependency topology: no C5 crate imports GPU

## Critères de sortie

- GPU absent : système fonctionne.
- GPU failure : no transaction impact.
- GPU result non validé : no StatsVersion publish.
- Aucune dépendance C5 vers GPU.

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
