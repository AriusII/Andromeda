# P03 — Catalogue, ContractHash et DefinitionBatch durable

## But

Rendre le catalogue réellement durable, transactionnel, versionné et récupérable, avec DefinitionBatch comme seule voie de mutation contrôlée.

## Pourquoi cette phase existe

andromeda-catalog possède déjà descriptors, contracts, DefinitionBatch, publication/subscription evidence et catalog WAL-facing codecs. Le risque est de laisser la vérité catalogue rester trop dispersée ou partiellement scaffold.

## Dépendances entrantes

- P01 specs CatalogObjectModel, DefinitionBatch, ContractHash.
- P02 vertical durable pour preuve de pattern.

## Objectifs

- Publier CatalogVersion seulement après evidence durable.
- Définir ContractHash canonique indépendant du formatage SRPL.
- Faire de DefinitionBatch un apply transactionnel avec rollback complet.
- Rendre recovery capable de rejouer seulement les batches complets et ordonnés.

## Tâches détaillées

- Spécifier et implémenter CatalogMutationRecord Begin/Apply/Commit.
- Vérifier dense apply indexes et source hash/dependency graph hash.
- Définir compatibility policy : additive, breaking, deprecated, rejected.
- Créer invalidation PlanCache liée à CatalogVersion/ContractHash.
- Créer catalog publication report pour Administration/HA-DR avec durable LSN evidence.
- Tester incomplete batch, duplicate apply, out-of-order apply, stale base catalog, contract breaking change.

## Livrables attendus

- CatalogStore v0 durable
- DefinitionBatch Apply v0
- ContractHash canonicalization v0
- CatalogRecoveryReport v0

## Personnes mobilisées

| Personne | Scope | Responsabilité |
| --- | --- | --- |
| Personne 03 | Lead Catalog, ContractHash et CatalogVersion | Structure le catalogue, les contrats, les versions, la compatibilité et les preuves de publication. |
| Personne 04 | Lead DefinitionBatch et migration contrôlée | Pilote DryRun, graphe de dépendances, rollback de batch, mutation catalog durable et audit. |
| Personne 08 | Lead WAL, FileWal, LSN et durabilité | Garantit FileWal, WAL codecs, durable prefix, flush_through, transaction classification et fences. |
| Personne 10 | Lead recovery, crash runner et forensic startup | Prouve REDO/UNDO, RecoveryReport, crash matrix, replay WAL et modes Online/ReadOnly/ForensicOnly. |
| Personne 13 | Lead IAM, principal, security admission et permissions | Gère CertificateIdentity, UserPrincipal, permissions, policies, deny/allow et admission fail-closed. |
| Personne 14 | Lead audit, observabilité et DecisionTrace | Garantit traces, audit ledger, corrélation, explanation post-mortem et métriques utiles. |

## Files d’attente de travail

| Queue | Contenu | Dépendance |
|---|---|---|
| Q1 — cadrage | Specs, ADR, invariants, critères de rejet. | Toujours en premier. |
| Q2 — preuve locale | Unit/property/golden tests du composant. | Q1 terminée. |
| Q3 — intégration | Liaison avec crates propriétaires et traces. | Q2 terminée. |
| Q4 — recovery/security | Crash/recovery ou security/admission si état durable ou externe. | Q3 terminée. |
| Q5 — documentation evidence | Rapport de validation et runbook si opérationnel. | Q4 terminée. |

## Tests et validations

- cargo test -p andromeda-catalog --test catalog_store_contract -- --nocapture
- cargo test -p andromeda-definition-batch --test alter_drop_compat -- --nocapture
- cargo test -p andromeda-catalog --test catalog_digest_contract -- --nocapture

## Critères de sortie

- Aucun demi-catalogue ne peut être visible.
- Recovery reconstruit la dernière CatalogVersion valide.
- ContractHash change seulement quand le contrat observable change.
- Toute publication catalog a durable evidence ou est rejetée.

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
