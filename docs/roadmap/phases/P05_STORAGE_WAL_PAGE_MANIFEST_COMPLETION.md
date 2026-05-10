# P05 — Storage V0 : pages, WAL, manifests et SegmentIndex

## But

Achever les primitives de stockage nécessaires au démarrage rapide, au replay et à la publication cold/hot cohérente.

## Pourquoi cette phase existe

andromeda-storage, storage-page, storage-heap, storage-index, disk-page-store, segment, manifest et buffer-pool existent. La phase doit unifier leurs preuves : PageLSN, WAL-before-page-flush, manifest switch et no native layout.

## Dépendances entrantes

- P01 specs Page/WAL/Manifest/SegmentIndex.
- P02/P03 evidence durable.

## Objectifs

- Faire respecter PageLSN <= durable WAL LSN avant flush.
- Stabiliser PageHeader/PageTrailer et codec explicite little-endian.
- Introduire root pointer, manifest chain et SegmentIndex minimal.
- Construire cold snapshot staging -> validation -> manifest switch.
- Éviter scan complet cold au startup.

## Tâches détaillées

- Implémenter ou stabiliser DiskPageStore + page codec golden vectors.
- Créer corruption rejection tests pour page, manifest, segment.
- Stabiliser BufferPool pin guards, dirty tracking, flush eligibility.
- Définir HotStore COW/log-structured V0 et ColdStore immutable.
- Implémenter manifest switch crash-safe avec evidence.
- Créer SegmentIndex locator minimal pour page and root B+Tree.

## Livrables attendus

- Page codec v1 stable
- Manifest v0 durable
- SegmentIndex v0 minimal
- Cold snapshot publication v0
- BufferPool durability fences

## Personnes mobilisées

| Personne | Scope | Responsabilité |
| --- | --- | --- |
| Personne 02 | Lead workspace Rust, topologie crates et CI | Garde Cargo, lints, dependency topology, cargo check/test/clippy/fmt et politiques supply-chain sous contrôle. |
| Personne 08 | Lead WAL, FileWal, LSN et durabilité | Garantit FileWal, WAL codecs, durable prefix, flush_through, transaction classification et fences. |
| Personne 09 | Lead storage pages, heap, BufferPool et manifests | Conçoit page/heap/index/store, PageLSN, manifest, SegmentIndex, snapshot froid et hot/cold storage. |
| Personne 10 | Lead recovery, crash runner et forensic startup | Prouve REDO/UNDO, RecoveryReport, crash matrix, replay WAL et modes Online/ReadOnly/ForensicOnly. |
| Personne 18 | Lead hardware CPU/NVMe performance | Pilote profils x64/arm64, SIMD runtime dispatch, NVMe queues, wear metrics et benchs mesurés. |

## Files d’attente de travail

| Queue | Contenu | Dépendance |
|---|---|---|
| Q1 — cadrage | Specs, ADR, invariants, critères de rejet. | Toujours en premier. |
| Q2 — preuve locale | Unit/property/golden tests du composant. | Q1 terminée. |
| Q3 — intégration | Liaison avec crates propriétaires et traces. | Q2 terminée. |
| Q4 — recovery/security | Crash/recovery ou security/admission si état durable ou externe. | Q3 terminée. |
| Q5 — documentation evidence | Rapport de validation et runbook si opérationnel. | Q4 terminée. |

## Tests et validations

- cargo test -p andromeda-storage-page --test page_codec_v1_contract -- --nocapture
- cargo test -p andromeda-storage-page --test property_page_codec_v1 -- --nocapture
- cargo test -p andromeda-storage --test recovery_contract -- --nocapture

## Critères de sortie

- Page flush without WAL coverage impossible.
- Manifest switch crash laisse ancien ou nouveau manifest valide.
- Startup lit root + manifest + SegmentIndex + WAL tail, pas tout le ColdStore.
- Aucun format durable ne dépend de struct Rust native.

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
