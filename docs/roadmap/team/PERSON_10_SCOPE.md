# Scope Personne 10 — Lead recovery, crash runner et forensic startup

## Mission

Prouve REDO/UNDO, RecoveryReport, crash matrix, replay WAL et modes Online/ReadOnly/ForensicOnly.

## Phases où cette personne est explicitement mobilisée

| Phase | Nom | But |
| --- | --- | --- |
| P01 | Spécifications normatives minimales | Transformer la doctrine en specs courtes, testables et refusables avant d’augmenter le périmètre fonctionnel. |
| P02 | Vertical durable Inventory.ProductStock | Prouver le chemin Procedure -> Admission -> Transaction -> WAL -> Heap/Page -> Recovery -> ResultStream sur un cas métier minimal. |
| P03 | Catalogue, ContractHash et DefinitionBatch durable | Rendre le catalogue réellement durable, transactionnel, versionné et récupérable, avec DefinitionBatch comme seule voie de mutation contrôlée. |
| P05 | Storage V0 : pages, WAL, manifests et SegmentIndex | Achever les primitives de stockage nécessaires au démarrage rapide, au replay et à la publication cold/hot cohérente. |
| P06 | Transaction, MVCC, isolation et rollback | Durcir la machine d’état transactionnelle, la visibilité MVCC, les conflits et les garanties d’isolation. |
| P13 | Backup, restore, PITR et forensic | Rendre backup et restore prouvables par artefacts, WAL archives, manifests et drills de restauration. |
| P14 | HA/DR Single Primary, quorum et fencing | Mettre en place HA/DR V0 single-primary avec WAL shipping, quorum, fencing et promotion prouvée. |
| P15 | Durcissement Enterprise Grade et release gates | Transformer le prototype recoverable en candidat de release interne avec preuves retenues, runbooks, supply chain et validation exhaustive. |

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
