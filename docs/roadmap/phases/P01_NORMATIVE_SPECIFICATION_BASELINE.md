# P01 — Spécifications normatives minimales

## But

Transformer la doctrine en specs courtes, testables et refusables avant d’augmenter le périmètre fonctionnel.

## Pourquoi cette phase existe

Les documents projet disent déjà que la prochaine étape utile est de produire ProcedureContract, TypeSystem, WalRecord, PageHeader, Manifest, FrameHeader, TransactionStateMachine, CatalogObjectModel et CrashRecoveryTestPlan. Cette phase rend ces specs normatives.

## Dépendances entrantes

- P00 validé.
- Doctrine Andromeda et documents SRPL/WAL/RPC disponibles.

## Objectifs

- Rendre chaque structure critique définie avant son extension.
- Fixer les invariants et les critères de rejet par spec.
- Créer des templates de spec réutilisables.
- Distinguer format durable, format réseau, IR sémantique et DTO runtime.

## Tâches détaillées

- Écrire ProcedureContract v0 : InputShape, OutputShape, ReadSet, WriteSet, RequiredPermissions, IsolationPolicy, ResourcePolicy, ProtocolLayout, CompatibilityPolicy.
- Écrire TypeSystem v0 : scalaires, decimal exact, float contrôlé, text policy, optional, cardinality.
- Écrire SRPL Grammar/Binder/IR v0 : absence explicite, cardinalité, set semantics, boucles bornées, diagnostics stables.
- Écrire WalRecord/FileWal v0 : LSN, PrevLsn, RecordLengthInv, CRC, ChainHash, TxBegin/RowInsert/TxCommit.
- Écrire PageHeader/PageTrailer/Manifest/SegmentIndex v0 : magic, versions, PageLSN, CRC/hash, no native Rust layout.
- Écrire RPC Frame/ResultStream v0 : metadata avant payload, frame type, stream role, errors typées.
- Écrire SecurityAdmission/AuditLedger v0 : mTLS identity, UserPrincipal, Permission, PolicyVersion, audit event.

## Livrables attendus

- `docs/specifications/SPEC_PROCEDURE_CONTRACT_V0.md`
- `docs/specifications/SPEC_TYPE_SYSTEM_V0.md`
- `docs/specifications/SPEC_SRPL_GRAMMAR_V0.md`
- `docs/specifications/SPEC_SRPL_BINDER_V0.md`
- `docs/specifications/SPEC_SEMANTIC_IR_V0.md`
- `docs/specifications/SPEC_CATALOG_OBJECT_MODEL_V0.md`
- `docs/specifications/SPEC_DEFINITION_BATCH_V0.md`
- `docs/specifications/SPEC_WAL_RECORD_V0.md`
- `docs/specifications/SPEC_FILE_WAL_SEGMENT_V0.md`
- `docs/specifications/SPEC_PAGE_FORMAT_V0.md`
- `docs/specifications/SPEC_DATABASE_MANIFEST_V0.md`
- `docs/specifications/SPEC_SEGMENT_INDEX_V0.md`
- `docs/specifications/SPEC_TRANSACTION_STATE_MACHINE_V0.md`
- `docs/specifications/SPEC_RECOVERY_REPORT_V0.md`
- `docs/specifications/SPEC_CRASH_RECOVERY_TEST_PLAN_V0.md`
- `docs/specifications/SPEC_RPC_FRAME_V0.md`
- `docs/specifications/SPEC_RESULT_STREAM_V0.md`
- `docs/specifications/SPEC_SECURITY_ADMISSION_V0.md`
- `docs/specifications/SPEC_AUDIT_LEDGER_V0.md`
- `docs/specifications/SPEC_DECISION_TRACE_V0.md`

## Personnes mobilisées

| Personne | Scope | Responsabilité |
| --- | --- | --- |
| Personne 01 | Lead doctrine, ADR et cohérence documentaire | Garantit que la roadmap, les ADR, les specs et les règles de rejet restent alignées avec la doctrine Andromeda. |
| Personne 03 | Lead Catalog, ContractHash et CatalogVersion | Structure le catalogue, les contrats, les versions, la compatibilité et les preuves de publication. |
| Personne 04 | Lead DefinitionBatch et migration contrôlée | Pilote DryRun, graphe de dépendances, rollback de batch, mutation catalog durable et audit. |
| Personne 05 | Lead SRPL parser, AST et diagnostics | Rend la syntaxe SRPL strictement bornée, stable, diagnostiquable et sans SQL dynamique. |
| Personne 06 | Lead SRPL binder, IR et cardinalité | Résout noms, types, cardinalités, absence explicite, read/write sets et Semantic IR. |
| Personne 08 | Lead WAL, FileWal, LSN et durabilité | Garantit FileWal, WAL codecs, durable prefix, flush_through, transaction classification et fences. |
| Personne 09 | Lead storage pages, heap, BufferPool et manifests | Conçoit page/heap/index/store, PageLSN, manifest, SegmentIndex, snapshot froid et hot/cold storage. |
| Personne 10 | Lead recovery, crash runner et forensic startup | Prouve REDO/UNDO, RecoveryReport, crash matrix, replay WAL et modes Online/ReadOnly/ForensicOnly. |
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

- Spec review checklist
- Golden vectors planned for codec specs
- No ambiguous terms : query, view, null ambient, dynamic SQL, native-layout serialization
- `python -B tools/testing/p01_spec_baseline_check.py --strict`

## Critères de sortie

- Chaque spec a invariants, erreurs, serialization, tests, rejection criteria.
- Toute structure durable ou réseau critique a un format explicite.
- Les personnes 3 à 14 peuvent démarrer leurs implémentations sans inventer la sémantique.
- `tools/testing/p01_spec_baseline_check.py --strict` passe sans gap.

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
