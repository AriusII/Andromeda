# Scope Personne 01 — Lead doctrine, ADR et cohérence documentaire

## Mission

Garantit que la roadmap, les ADR, les specs et les règles de rejet restent alignées avec la doctrine Andromeda.

## Phases où cette personne est explicitement mobilisée

| Phase | Nom | But |
| --- | --- | --- |
| P00 | État dépôt, baseline Rust et gouvernance des preuves | Stabiliser la lecture du dépôt main, figer la baseline Rust 1.95.0, corriger les incohérences documentaires et créer les règles de preuve communes. |
| P01 | Spécifications normatives minimales | Transformer la doctrine en specs courtes, testables et refusables avant d’augmenter le périmètre fonctionnel. |
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
