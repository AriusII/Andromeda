# P14 — HA/DR Single Primary, quorum et fencing

## But

Mettre en place HA/DR V0 single-primary avec WAL shipping, quorum, fencing et promotion prouvée.

## Pourquoi cette phase existe

andromeda-hadr est un scaffold. La doctrine rejette le multi-primary V0 et impose quorum/fencing pour éviter split-brain.

## Dépendances entrantes

- P13 backup/PITR.
- P05 WAL/manifest.
- P07 security/audit.
- P08 QUIC HA/DR surface.

## Objectifs

- Créer ClusterManifest avec Epoch, PrimaryNodeId, Members, QuorumPolicy, FencingTokensHash.
- Implémenter WAL shipping evidence.
- Refuser promotion sans quorum, fencing et LSN validation.
- Créer replica catch-up et resync snapshot.

## Tâches détaillées

- Définir Node metadata : role, priority, last durable/applied LSN, lag, eligibility.
- Créer HA/DR stream ranges et mTLS cluster scopes.
- Tester primary suspect -> quorum -> fencing -> recovery -> promotion -> manifest update.
- Créer split-brain negative tests.
- Créer WAL shipping sync/async modes avec RPO policy.
- Créer cluster event audit.

## Livrables attendus

- ClusterManifest v0
- WalShippingEvidence v0
- PromotionReport v0
- FencingPolicy v0
- HA/DR runbooks

## Personnes mobilisées

| Personne | Scope | Responsabilité |
| --- | --- | --- |
| Personne 08 | Lead WAL, FileWal, LSN et durabilité | Garantit FileWal, WAL codecs, durable prefix, flush_through, transaction classification et fences. |
| Personne 10 | Lead recovery, crash runner et forensic startup | Prouve REDO/UNDO, RecoveryReport, crash matrix, replay WAL et modes Online/ReadOnly/ForensicOnly. |
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

- promotion without quorum refused
- stale epoch rejected
- replica lag incompatible -> no promotion
- WAL shipping range validation
- fencing token validation

## Critères de sortie

- Aucune auto-promotion sans quorum/fencing.
- Un old primary epoch obsolète est refusé.
- Le failover a ClusterEventTrace complet.
- Backup reste distinct de HA/DR.

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
