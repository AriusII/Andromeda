# Scope Personne 18 — Lead hardware CPU/NVMe performance

## Mission

Pilote profils x64/arm64, SIMD runtime dispatch, NVMe queues, wear metrics et benchs mesurés.

## Phases où cette personne est explicitement mobilisée

| Phase | Nom | But |
| --- | --- | --- |
| P05 | Storage V0 : pages, WAL, manifests et SegmentIndex | Achever les primitives de stockage nécessaires au démarrage rapide, au replay et à la publication cold/hot cohérente. |
| P09 | Statistics, Procedure Store et optimizer V0 | Introduire des statistiques versionnées, un coût V0 explicable et un optimizer borné sans composants learned autoritaires. |
| P11 | Performance CPU/NVMe et gouvernance ressources | Optimiser seulement après preuve fonctionnelle, avec métriques, profils hardware, fallback et policies. |
| P12 | GPU batch optionnel | Ajouter GPU_STATS/GPU_ANALYTICS/GPU_BENCHMARK seulement comme accélération batch optionnelle, jamais C5. |
| P15 | Durcissement Enterprise Grade et release gates | Transformer le prototype recoverable en candidat de release interne avec preuves retenues, runbooks, supply chain et validation exhaustive. |

## Dépendances entrantes

- P02-P08 doivent fournir truth, catalog, execution, storage, transaction, security et RPC stables.

## Dépendances sortantes

- P15 consomme les preuves produites par ce scope.

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
