# P04 — Exécution générique de Procedures cataloguées

## But

Remplacer les chemins hard-codés par un dispatch ProcedureId + ContractHash + CatalogVersion + payload shape.

## Pourquoi cette phase existe

Le vertical Inventory reste utile mais ne doit pas devenir l’unique chemin. Le dépôt possède andromeda-exec, andromeda-procedure-runtime, andromeda-execution, SRPL adapters et ResultStream. Il faut généraliser sans ouvrir de surface SQL.

## Dépendances entrantes

- P02 durable vertical.
- P03 catalog/contracts durables.
- P01 RPC/ProcedureContract specs.

## Objectifs

- Créer un ProcedureRegistry catalog-backed.
- Valider admission, contract binding, payload, resource budget avant transaction.
- Appeler SRPL IR/execution adapter plutôt qu’un handler hard-coded.
- Unifier erreurs business, contract, permission, transaction, resource, system.

## Tâches détaillées

- Créer ProcedureDispatchRequest canonique.
- Brancher ProcedureContractBinding dans execution context.
- Écrire adapter minimal pour deux Procedures indépendantes.
- Renforcer ResultStream metadata/payload/completion pour tous résultats.
- Gérer retries/deadlock/timeout avec rollback fence durable.
- Émettre ProcedureInvocationTrace et ExecutionTransitionTrace.

## Livrables attendus

- ProcedureRegistry v0
- Generic Procedure dispatch path
- Typed error conversion matrix
- ExecutionTrace v0

## Personnes mobilisées

| Personne | Scope | Responsabilité |
| --- | --- | --- |
| Personne 03 | Lead Catalog, ContractHash et CatalogVersion | Structure le catalogue, les contrats, les versions, la compatibilité et les preuves de publication. |
| Personne 05 | Lead SRPL parser, AST et diagnostics | Rend la syntaxe SRPL strictement bornée, stable, diagnostiquable et sans SQL dynamique. |
| Personne 06 | Lead SRPL binder, IR et cardinalité | Résout noms, types, cardinalités, absence explicite, read/write sets et Semantic IR. |
| Personne 07 | Lead admission et Procedure execution runtime | Assure la chaîne Procedure -> contract binding -> permission -> transaction -> completion. |
| Personne 12 | Lead QUIC/RPC/protocol/runtime | Sépare transport QUIC et sémantique RPC, frame sequencing, ResultStream et runtime Quinn. |
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

- cargo test -p andromeda-exec --test runtime_contract -- --nocapture
- cargo test -p andromeda-execution --test srpl_adapter_contract -- --nocapture
- cargo test -p andromeda-result-stream --test result_stream_backpressure -- --nocapture

## Critères de sortie

- Deux Procedures cataloguées distinctes s’exécutent sans handler spécial.
- Aucun appel ne crée transaction avant admission complète.
- Les erreurs sont typées et auditables.
- Aucun SQL texte ou dynamic predicate n’entre dans le dispatch.

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
