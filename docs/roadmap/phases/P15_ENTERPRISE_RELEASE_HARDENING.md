# P15 — Durcissement Enterprise Grade et release gates

## But

Transformer le prototype recoverable en candidat de release interne avec preuves retenues, runbooks, supply chain et validation exhaustive.

## Pourquoi cette phase existe

Le README GitHub dit que la readiness se juge par status docs, release-gate evidence et validation output, pas par ADR historiques. Cette phase clôt le chemin en exigeant des preuves opérationnelles.

## Dépendances entrantes

- P00-P14 exit criteria.

## Objectifs

- Créer release evidence bundle.
- Valider tout workspace.
- Figer runbooks incident WAL, client lent, replica lag, corruption, backup/restore, security incident.
- Créer supply chain gates cargo audit/deny/vet.
- Documenter non-production vs production claims.

## Tâches détaillées

- Créer ReleaseReadiness.md avec preuve par phase.
- Exécuter cargo fmt/check/test/clippy workspace.
- Exécuter fuzz/Miri ciblés pour codecs et unsafe.
- Exécuter crash/recovery suite complète.
- Exécuter backup/restore/PITR drills.
- Vérifier audit ledger, security admission, QUIC surfaces, HA/DR negative tests.
- Produire known gaps et non-goals restants.

## Livrables attendus

- ReleaseEvidenceBundle
- Runbook pack Enterprise
- KnownGaps.md
- ProductionReadinessBoundary.md
- SupplyChainReport

## Personnes mobilisées

| Personne | Scope | Responsabilité |
| --- | --- | --- |
| Personne 01 | Lead doctrine, ADR et cohérence documentaire | Garantit que la roadmap, les ADR, les specs et les règles de rejet restent alignées avec la doctrine Andromeda. |
| Personne 02 | Lead workspace Rust, topologie crates et CI | Garde Cargo, lints, dependency topology, cargo check/test/clippy/fmt et politiques supply-chain sous contrôle. |
| Personne 03 | Lead Catalog, ContractHash et CatalogVersion | Structure le catalogue, les contrats, les versions, la compatibilité et les preuves de publication. |
| Personne 08 | Lead WAL, FileWal, LSN et durabilité | Garantit FileWal, WAL codecs, durable prefix, flush_through, transaction classification et fences. |
| Personne 09 | Lead storage pages, heap, BufferPool et manifests | Conçoit page/heap/index/store, PageLSN, manifest, SegmentIndex, snapshot froid et hot/cold storage. |
| Personne 10 | Lead recovery, crash runner et forensic startup | Prouve REDO/UNDO, RecoveryReport, crash matrix, replay WAL et modes Online/ReadOnly/ForensicOnly. |
| Personne 12 | Lead QUIC/RPC/protocol/runtime | Sépare transport QUIC et sémantique RPC, frame sequencing, ResultStream et runtime Quinn. |
| Personne 13 | Lead IAM, principal, security admission et permissions | Gère CertificateIdentity, UserPrincipal, permissions, policies, deny/allow et admission fail-closed. |
| Personne 14 | Lead audit, observabilité et DecisionTrace | Garantit traces, audit ledger, corrélation, explanation post-mortem et métriques utiles. |
| Personne 15 | Lead optimizer, PlanCacheKey et plan classes | Structure coût V0, bounded candidates, PlanClass, hysteresis et DecisionTrace optimizer. |
| Personne 16 | Lead statistics, Procedure Store et feedback | Publie StatsVersion candidates, histogrammes, skew, NDV, Procedure Store et regression detection. |
| Personne 17 | Lead Maps, analytics CPU et columnar | Déclare grain, summarizability, Map refresh modes, column chunks et analytics CPU avant GPU. |
| Personne 18 | Lead hardware CPU/NVMe performance | Pilote profils x64/arm64, SIMD runtime dispatch, NVMe queues, wear metrics et benchs mesurés. |
| Personne 19 | Lead GPU batch optionnel | Isole GPU_STATS/GPU_ANALYTICS/GPU_BENCHMARK avec fallback CPU et kill switches hors C5. |
| Personne 20 | Lead backup, restore, HA/DR et release operations | Pilote PITR, backup validation, WAL archives, quorum/fencing, runbooks, release gates et drills. |

## Files d’attente de travail

| Queue | Contenu | Dépendance |
|---|---|---|
| Q1 — cadrage | Specs, ADR, invariants, critères de rejet. | Toujours en premier. |
| Q2 — preuve locale | Unit/property/golden tests du composant. | Q1 terminée. |
| Q3 — intégration | Liaison avec crates propriétaires et traces. | Q2 terminée. |
| Q4 — recovery/security | Crash/recovery ou security/admission si état durable ou externe. | Q3 terminée. |
| Q5 — documentation evidence | Rapport de validation et runbook si opérationnel. | Q4 terminée. |

## Tests et validations

- cargo fmt --all -- --check
- cargo check --workspace --locked
- cargo test --workspace --locked
- cargo clippy --workspace --all-targets --locked -- -D warnings
- cargo audit && cargo deny check && cargo vet
- crash/recovery/restore/forensic gates

## Critères de sortie

- Les claims de readiness sont strictement alignés avec les preuves.
- Aucun C5 gap critique ouvert sans mitigation.
- Les runbooks couvrent les incidents majeurs.
- Le projet peut passer en étape suivante avec une base stable, pas seulement prometteuse.

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
