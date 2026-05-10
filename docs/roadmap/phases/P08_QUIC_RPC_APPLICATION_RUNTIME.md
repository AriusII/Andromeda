# P08 — QUIC/RPC runtime applicatif

## But

Activer un runtime QUIC applicatif feature-gated après stabilisation admission, frames et Procedure dispatch.

## Pourquoi cette phase existe

andromeda-quic est runtime-free et andromeda-quic-runtime-quinn porte Quinn/Rustls/Tokio. Cette séparation est saine ; l’objectif est de l’utiliser sans laisser le transport décider de la sémantique Procedure.

## Dépendances entrantes

- P04 Procedure dispatch.
- P07 IAM admission durable.
- P01 RPC frame spec.

## Objectifs

- Maintenir QUIC = transport et RPC = sémantique.
- Valider stream roles, frame envelopes, payload lengths et ResultStream ordering.
- Refuser gRPC/tonic/HTTP2 service surface.
- Appliquer backpressure sans starver WAL.

## Tâches détaillées

- Feature-gater runtime-quinn.
- Créer mTLS dev certificates et cert scope validation.
- Tester Application, Administration, HA/DR stream ranges.
- Valider RpcExecuteRequest : no client-supplied TxId, contract hash, manifest, versions.
- Implémenter slow client backpressure et bounded stream buffers.
- Créer malformed frame fuzz/property tests.

## Livrables attendus

- Application QUIC runtime v0
- RPC frame sequence validators
- Backpressure policy v0
- Surface separation tests

## Personnes mobilisées

| Personne | Scope | Responsabilité |
| --- | --- | --- |
| Personne 02 | Lead workspace Rust, topologie crates et CI | Garde Cargo, lints, dependency topology, cargo check/test/clippy/fmt et politiques supply-chain sous contrôle. |
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

- cargo test -p andromeda-quic --test procedure_gateway_route -- --nocapture
- cargo test -p andromeda-quic-runtime-quinn --test real_quinn_network -- --nocapture
- cargo test -p andromeda-rpc-protocol --all-targets

## Critères de sortie

- Admin frame rejetée sur Application Surface.
- Result metadata précède toujours payload.
- Payload oversized rejeté avant allocation dangereuse.
- Transport events ne modifient pas la sémantique Procedure.

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
