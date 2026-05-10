# P00 — État dépôt, baseline Rust et gouvernance des preuves

## But

Stabiliser la lecture du dépôt main, figer la baseline Rust 1.95.0, corriger les incohérences documentaires et créer les règles de preuve communes.

## Pourquoi cette phase existe

Le Cargo actuel montre Rust 1.95.0 et 89 crates, tandis que certains textes historiques mentionnent encore 88 crates. La roadmap doit partir de la vérité repo, pas d’un souvenir documentaire.

## Dépendances entrantes

- Aucune dépendance technique ; dépend de la capacité à lire Cargo.toml, README, docs et crates README.

## Objectifs

- Établir un Current State Report qui sépare implémenté, scaffold, spécifié, prototype et non commencé.
- Mettre à jour la baseline documentaire : Rust 1.95.0, Rust 2024 Edition, resolver 3, 89 crates.
- Créer une grille commune de criticité C0 à C5 et une règle d’évidence par phase.
- Interdire les claims de production readiness non prouvés.

## Tâches détaillées

- Compter les crates depuis Cargo.toml et enregistrer la liste de crates par cluster.
- Comparer README, docs/README, roadmap et Cargo.toml pour détecter les divergences.
- Créer ou mettre à jour ADR Rust baseline, crate boundaries, no SQL, no gRPC, no native-layout serialization.
- Écrire la règle de preuve : code existant + tests ciblés + trace + recovery behavior si état durable.
- Créer un registre des incohérences : docs indiquant 88 crates, status.md absent, docs remplacés, roadmap P0-P11 trop générique.

## Livrables attendus

- docs/roadmap/00_CURRENT_STATE_CROSS_CHECK.md
- docs/roadmap/sources/CROSS_CHECK_SOURCES.md
- ADR Rust baseline et workspace count
- Matrice crate -> engine cluster -> criticité

## Personnes mobilisées

| Personne | Scope | Responsabilité |
| --- | --- | --- |
| Personne 01 | Lead doctrine, ADR et cohérence documentaire | Garantit que la roadmap, les ADR, les specs et les règles de rejet restent alignées avec la doctrine Andromeda. |
| Personne 02 | Lead workspace Rust, topologie crates et CI | Garde Cargo, lints, dependency topology, cargo check/test/clippy/fmt et politiques supply-chain sous contrôle. |
| Personne 14 | Lead audit, observabilité et DecisionTrace | Garantit traces, audit ledger, corrélation, explanation post-mortem et métriques utiles. |
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
- cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture

## Critères de sortie

- Rust 1.95.0 et 89 crates sont la base officielle de cette roadmap.
- Toutes les divergences documentaires ont un statut : corriger, garder historique, ou retirer de navigation.
- Chaque phase de roadmap utilise entry criteria, exit criteria, dépendances et acceptance checks.

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
