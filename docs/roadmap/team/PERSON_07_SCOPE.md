# Scope Personne 07 — Lead admission et Procedure execution runtime

## Mission

Assure la chaîne Procedure -> contract binding -> permission -> transaction -> completion.

## Phases où cette personne est explicitement mobilisée

| Phase | Nom | But |
| --- | --- | --- |
| P02 | Vertical durable Inventory.ProductStock | Prouver le chemin Procedure -> Admission -> Transaction -> WAL -> Heap/Page -> Recovery -> ResultStream sur un cas métier minimal. |
| P04 | Exécution générique de Procedures cataloguées | Remplacer les chemins hard-codés par un dispatch ProcedureId + ContractHash + CatalogVersion + payload shape. |
| P06 | Transaction, MVCC, isolation et rollback | Durcir la machine d’état transactionnelle, la visibilité MVCC, les conflits et les garanties d’isolation. |
| P08 | QUIC/RPC runtime applicatif | Activer un runtime QUIC applicatif feature-gated après stabilisation admission, frames et Procedure dispatch. |

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
