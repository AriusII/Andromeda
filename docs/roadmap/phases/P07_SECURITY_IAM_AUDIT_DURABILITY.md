# P07 — IAM, sécurité et audit durable

## But

Persister les identités, certificats, policies et audit evidence sans créer de bypass applicatif.

## Pourquoi cette phase existe

andromeda-iam possède l’admission pré-transaction fail-closed, andromeda-security est un scaffold, andromeda-audit/observe exposent l’evidence. Cette phase transforme le runtime minimal en gouvernance durable.

## Dépendances entrantes

- P03 catalog durable.
- P04 admission Procedure.
- P05 storage/audit durable support.

## Objectifs

- Séparer CertificateIdentity, UserPrincipal, roles, permissions, policies.
- Créer durable registries avec WAL coverage.
- Émettre SecurityAuditTrace pour every allow/deny.
- Créer break-glass borné, temporisé, auditable.
- Garantir deny explicite prioritaire sauf break-glass formel.

## Tâches détaillées

- Spécifier security registries et policies versionnées.
- Brancher IAM admission à cataloged Procedure permissions.
- Créer audit ledger append-only checksum chained.
- Tester disabled principal, revoked certificate, missing permission, wrong surface.
- Créer DTO audit sans dependency inversion vers observe inappropriée.
- Définir retention et compaction audit qui ne devient pas storage truth.

## Livrables attendus

- UserPrincipal Registry v0
- CertificateIdentity Registry v0
- PolicyVersion v0
- AuditLedger v0
- BreakGlassPolicy v0

## Personnes mobilisées

| Personne | Scope | Responsabilité |
| --- | --- | --- |
| Personne 03 | Lead Catalog, ContractHash et CatalogVersion | Structure le catalogue, les contrats, les versions, la compatibilité et les preuves de publication. |
| Personne 12 | Lead QUIC/RPC/protocol/runtime | Sépare transport QUIC et sémantique RPC, frame sequencing, ResultStream et runtime Quinn. |
| Personne 13 | Lead IAM, principal, security admission et permissions | Gère CertificateIdentity, UserPrincipal, permissions, policies, deny/allow et admission fail-closed. |
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

- cargo test -p andromeda-iam --all-targets
- cargo test -p andromeda-security --all-targets
- cargo test -p andromeda-audit --all-targets
- security admission fail-closed tests

## Critères de sortie

- Aucun denied request ne reçoit de receipt.
- Aucune transaction créée avant admission.
- Every security decision emits audit evidence.
- Application Surface ne peut pas porter admin/cluster actions.

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
