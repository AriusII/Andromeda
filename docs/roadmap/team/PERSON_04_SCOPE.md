# Scope Personne 04 — Lead DefinitionBatch et migration contrôlée

## Mission

Pilote DryRun, graphe de dépendances, rollback de batch, mutation catalog durable et audit.

## Phases où cette personne est explicitement mobilisée

| Phase | Nom | But |
| --- | --- | --- |
| P01 | Spécifications normatives minimales | Transformer la doctrine en specs courtes, testables et refusables avant d’augmenter le périmètre fonctionnel. |
| P03 | Catalogue, ContractHash et DefinitionBatch durable | Rendre le catalogue réellement durable, transactionnel, versionné et récupérable, avec DefinitionBatch comme seule voie de mutation contrôlée. |

## Dépendances entrantes

- P00/P01 doivent fournir specs et ADR.

## Dépendances sortantes

- P09-P15 dépendent de la stabilité de ce scope C5.

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
