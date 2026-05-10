# Scope Personne 02 — Lead workspace Rust, topologie crates et CI

## Mission

Garde Cargo, lints, dependency topology, cargo check/test/clippy/fmt et politiques supply-chain sous contrôle.

## Phases où cette personne est explicitement mobilisée

| Phase | Nom | But |
| --- | --- | --- |
| P00 | État dépôt, baseline Rust et gouvernance des preuves | Stabiliser la lecture du dépôt main, figer la baseline Rust 1.95.0, corriger les incohérences documentaires et créer les règles de preuve communes. |
| P02 | Vertical durable Inventory.ProductStock | Prouver le chemin Procedure -> Admission -> Transaction -> WAL -> Heap/Page -> Recovery -> ResultStream sur un cas métier minimal. |
| P05 | Storage V0 : pages, WAL, manifests et SegmentIndex | Achever les primitives de stockage nécessaires au démarrage rapide, au replay et à la publication cold/hot cohérente. |
| P08 | QUIC/RPC runtime applicatif | Activer un runtime QUIC applicatif feature-gated après stabilisation admission, frames et Procedure dispatch. |
| P11 | Performance CPU/NVMe et gouvernance ressources | Optimiser seulement après preuve fonctionnelle, avec métriques, profils hardware, fallback et policies. |
| P12 | GPU batch optionnel | Ajouter GPU_STATS/GPU_ANALYTICS/GPU_BENCHMARK seulement comme accélération batch optionnelle, jamais C5. |
| P15 | Durcissement Enterprise Grade et release gates | Transformer le prototype recoverable en candidat de release interne avec preuves retenues, runbooks, supply chain et validation exhaustive. |

## Dépendances entrantes

- Aucune dépendance globale autre que la doctrine projet.

## Dépendances sortantes

- Toutes les phases : gouvernance, CI, preuves et coherence documentaire.

## Travail attendu

- Lire `ROADMAP_MASTER.md` et le fichier de phase avant de proposer du code.
- Produire des changements petits, traçables et reliés à un owner crate.
- Ajouter tests ciblés avant de réclamer la sortie de phase.
- Documenter les gaps résiduels dans un rapport de validation.
- Ne jamais compenser une faiblesse de design par une permissivité runtime.

## Preuves minimales par PR

| Type de changement | Evidence minimale |
|---|---|
| Spec ou ADR | Invariants, rejection criteria, liens vers phases concernées. |
| Code C5 | Unit + property/golden + crash/recovery ou fail-closed selon zone. |
| Protocol/security | Negative tests et audit/trace evidence. |
| Adaptive/performance | Benchmark, fallback, disablement, DecisionTrace. |
| Documentation | No calendar promise, current-state alignment, liens stables. |
