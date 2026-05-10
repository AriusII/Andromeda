# P13 — Backup, restore, PITR et forensic

## But

Rendre backup et restore prouvables par artefacts, WAL archives, manifests et drills de restauration.

## Pourquoi cette phase existe

andromeda-backup et andromeda-restore sont actuellement des scaffolds. La doctrine dit qu’une replica n’est pas un backup et qu’un backup non testé est une hypothèse.

## Dépendances entrantes

- P05 storage/manifest/snapshot.
- P07 audit/security.
- P10 maps rebuild evidence.

## Objectifs

- Créer BackupManifest et RestorePlan v0.
- Conserver snapshot froid, WAL archives, catalog, manifests, audit ledger.
- Valider PITR par LSN cible.
- Créer ForensicStart report qui bloque Application Surface.

## Tâches détaillées

- Définir backup artifacts inventory, checkpoint evidence, WAL archive coverage.
- Implémenter restore validation : manifest/hash/signature, SegmentIndex, WAL range.
- Tester corruption, missing artifact, incomplete WAL coverage.
- Créer read-only restore drill.
- Créer retention et immutability policy.
- Créer forensic consistency report : catalog, WAL, snapshot, indexes, maps, security audit.

## Livrables attendus

- BackupManifest v0
- RestorePlan v0
- PITR LSN restore v0
- ForensicStartReport v0
- Restore drill runbook

## Personnes mobilisées

| Personne | Scope | Responsabilité |
| --- | --- | --- |
| Personne 08 | Lead WAL, FileWal, LSN et durabilité | Garantit FileWal, WAL codecs, durable prefix, flush_through, transaction classification et fences. |
| Personne 09 | Lead storage pages, heap, BufferPool et manifests | Conçoit page/heap/index/store, PageLSN, manifest, SegmentIndex, snapshot froid et hot/cold storage. |
| Personne 10 | Lead recovery, crash runner et forensic startup | Prouve REDO/UNDO, RecoveryReport, crash matrix, replay WAL et modes Online/ReadOnly/ForensicOnly. |
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

- snapshot + WAL replay restore test
- PITR exact LSN test
- missing WAL coverage fails closed
- ForensicStart blocks app traffic

## Critères de sortie

- Aucun backup ne marque recoverable sans restore evidence.
- Restore success dépend seulement durable artifacts.
- PITR cible rejoue jusqu’au LSN prévu.
- ForensicStart produit rapport et bloque trafic applicatif.

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
