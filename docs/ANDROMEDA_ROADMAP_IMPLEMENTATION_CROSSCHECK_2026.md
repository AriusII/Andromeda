# Andromeda — Roadmap d’implémentation consolidée et cross-check GitHub/Sources

**Date de consolidation :** 2026-05-06  
**Dépôt vérifié :** `AriusII/Andromeda`, branche `main`  
**Livrable :** Roadmap Markdown versionnable dans le dépôt  
**But :** transformer la doctrine Andromeda en plan d’implémentation fin, priorisé, testable et gouverné.

---

## 1. Position de synthèse

Andromeda a déjà franchi le stade du simple brainstorming : le dépôt contient un workspace Rust structuré, des crates alignées sur les moteurs cibles, une tranche verticale locale `Inventory.ReserveStock`, un WAL fichier V0, des tests de recovery, un protocole de frames/RPC local, des contrats de procédure, des structures de statistics/histograms, un plan-cache scaffold, un scenario-evidence scaffold, un benchmark engine diagnostique borné, des premières surfaces HA/DR/backup/restore, et une couche documentaire/agentique de gouvernance.

La conclusion opérationnelle est cependant stricte : **le projet n’est pas encore un SGBDRT production**. C’est un **prototype vertical recoverable + fondation modulaire avancée**. Les prochaines phases doivent convertir les scaffolds et contrats en moteurs runtime intégrés : catalogue durable, DefinitionBatch réel, SRPL complet, optimizer minimal réel, storage durable multi-page, MVCC exploitable sur tables, QUIC serveur/client réel, IAM complet, puis analytics/GPU hors chemin de commit.

La doctrine principale reste cohérente entre les sources et le dépôt : **strict aux frontières, adaptatif à l’intérieur**. Le noyau critique reste : pas de SQL ad hoc applicatif, exécution par Procedure, contrat typé et versionné, transaction implicite, commit visible uniquement après WAL durable, vérité reconstruite par snapshot froid + WAL, GPU strictement hors commit/recovery, evidence non décisionnaire seule, observabilité obligatoire.

---

## 2. Sources croisées utilisées

### 2.1 Sources doctrinales consolidées

Les sources internes établissent les invariants non négociables : pas de SQL ad hoc, Procedure cataloguée obligatoire, contrat typé/hashé/versionné, transaction implicite, WAL durable avant commit visible, vérité `Cold Snapshot + WAL`, GPU hors commit path, benchmark prédictif non décisionnaire, plans liés à `CatalogVersion + StatsVersion + ContractHash` fileciteturn0file7. Le document doctrine/architecture précise les moteurs fonctionnels, les plans transverses, le catalogue, la Modelization et les interdictions de dépendance, notamment `GPU Runtime -> Commit Protocol`, `Predictive Evidence Engine -> Forced Plan Decision` et `Map Refresh -> WAL Priority Override` fileciteturn0file5.

Le document Type System/SRPL fixe l’identité du langage : typage fort, absence explicite au lieu de `NULL`, cardinalité déclarée (`one`, `optional one`, `many`, `nonempty many`), set semantics par défaut, agrégats explicites sur cas vide, StructuredObjects typés, Procedures transactionnelles, Maps matérialisées et boucles bornées uniquement fileciteturn0file6. Le PDF SRPL renforce cette ligne : SRPL ne doit pas être un SQL renommé, mais un langage procédural relationnel strict avec syntaxe lisible, portée stricte, absence typée, cardinalité explicite, compilation vers IR et plan cache sémantique fileciteturn0file2.

Le document transaction/storage fixe l’invariant C5 : `Commit visible = WAL durable`, la vérité système `dernier Cold Snapshot valide + WAL durable`, le rôle non-vérité de RAM/NVMe HotStore/GPU, le pipeline d’écriture, le recovery depuis manifest/snapshot/WAL, les pages, extents, segments froids immutables et l’I/O scheduler où le WAL flush est P0 fileciteturn0file4. Le document QUIC/RPC/Security/HA précise les trois surfaces QUIC séparées, les frames typées, les metadata avant payload, la chaîne `mTLS -> CertificateIdentity -> UserPrincipal -> Roles/Permissions/Policies -> Audit`, HA/DR single-primary V0, quorum/fencing, WAL shipping, backup/PITR et ForensicStart fileciteturn0file3.

Le document optimizer/statistics/hardware/roadmap fixe la règle d’adaptation : bornée, versionnée, observable, explicable, désactivable. Il place le Procedure Store, l’Optimizer, le Statistics Engine, le Predictive Evidence Engine, les Maps analytiques et le GPU dans un plan adaptatif, mais interdit au GPU le commit, WAL, rollback, recovery, index lookup ligne par ligne, logique OLTP courte et sécurité critique fileciteturn0file8.

### 2.2 Sources scientifiques externes intégrées

Le rapport scientifique rappelle que le socle durable reste relationnel : modèle de Codd, algèbre relationnelle, optimisation à coût, transactions, WAL/ARIES, normalisation, statistiques, B/B+Trees. Les composants learned restent sérieux mais non universels ; ils doivent être validés empiriquement et protégés contre drift/régression/poisoning fileciteturn0file0. Le corpus de référence distingue explicitement consensus scientifique durable et front moderne 2024-2026 : coût/cardinalité/statistiques restent centraux ; learned CE/indexes/optimizers sont à traiter comme recherche active et non comme remplacement universel fileciteturn0file1.

### 2.3 Sources GitHub vérifiées

Le `Cargo.toml` racine expose un workspace Rust édition 2024 avec les crates `andromeda-bench`, `andromeda-cli`, `andromeda-catalog`, `andromeda-core`, `andromeda-exec`, `andromeda-observe`, `andromeda-proto`, `andromeda-quic`, `andromeda-srpl`, `andromeda-storage`, `andromeda-tx`, et les dépendances QUIC/TLS `quinn`, `rcgen`, `rustls`, `tokio` fileciteturn5file0. Le README définit le dépôt comme un AI operating pack et un workspace Rust V0 local : `vertical-v0`, `recovery-inspect`, `protocol-smoke`; il précise explicitement que ce n’est pas encore un runtime production, un serveur réseau complet ou un storage engine complet fileciteturn6file0.

DEC-011 accepte la topologie workspace Rust et les frontières core/proto/quic/catalog/srpl/tx/storage/exec/observe/cli fileciteturn60file0. DEC-012 accepte les baselines Phase 0 : descriptors, catalog objects, contracts, Protobuf boundary envelopes, QUIC frame policy, SRPL semantics, transaction state, WAL records, page metadata, manifest, execution admission et observability traces fileciteturn61file0. DEC-013 accepte le prototype vertical local avec `Inventory.ReserveStock`, validation contrat avant transaction, WAL flush avant commit visible et result metadata avant payload fileciteturn62file0. DEC-015 accepte la tranche métier `Inventory.ReserveStock`, avec product identity, positive quantity, stock suffisant, exact result evidence et payload déterministe fileciteturn64file0.

Le commit `accbe16` indique une extension récente très large : statistics engine avec histogrammes et publication `StatsVersion`, WAL record types catalog mutations, plan cache invalidation sur stats, entrée CLI backup/catalog/hadr/restore, hardware profiles, executor bridge, admission pipeline, result streaming, observe event families, proto manifest/structured object support, QUIC backpressure/frame/RPC dispatch/stream concurrency, SRPL definition batch bridge/interpreter/lowering/parser, backup/checkpoint/WAL archive/HADR shipping, MVCC status et transaction state fileciteturn44file0. Le commit `e0b743` indique aussi un nettoyage massif d’artefacts docs obsolètes et confirme que certains documents Wave historiques ont été supprimés ; les références vivantes sont donc les fichiers de code, README, DEC encore présentes et docs actuelles fileciteturn45file0.

---

## 3. État réel du dépôt `main`

### 3.1 Ce qui est effectivement en place

| Domaine | État observé | Preuve GitHub | Lecture opérationnelle |
|---|---|---|---|
| Workspace Rust | Workspace multi-crate édition 2024 | `Cargo.toml` racine fileciteturn5file0 | Structure alignée avec la doctrine moteurs. |
| Core | Identifiants, erreurs, temps, types, principals, hardware CPU/GPU/RAM, policies ; `unsafe_code` interdit | `andromeda-core/src/lib.rs` fileciteturn17file0 | Base saine pour contrats inter-crates. |
| GPU policy | `GpuExecutionPolicy::{Disabled, OffCriticalPathOnly, BatchAnalyticsOnly}`, refus commit/WAL/rollback/recovery | `hardware_gpu.rs` fileciteturn36file0 | GPU déjà borné doctrinalement, pas encore moteur runtime GPU. |
| Pipeline classes | Commit, WAL, rollback, recovery, foreground, maintenance, statistics, map refresh, batch analytics | `hardware_pipeline.rs` fileciteturn37file0 | Bon socle pour governance hardware. |
| Catalog | Objects, contracts, DefinitionBatch, procedure store, plan cache, scenario evidence, statistics, WAL integration | `andromeda-catalog/src/lib.rs` fileciteturn22file0 | Beaucoup de surface existe ; intégration runtime encore à durcir. |
| Procedure contract | `ProcedureContract`, `StatsVersion`, `PolicyVersion`, `ProcedureContractBinding`, protocol layout, result metadata, policies | `contracts/types.rs` fileciteturn35file0 | Contrat très proche de la doctrine. |
| Plan cache | Scaffold seulement, clé déterministe, PlanClass bornées, invalidation par versions | `plan_cache.rs` fileciteturn50file0 | Pas encore optimizer/cache runtime. |
| ScenarioEvidence | Scaffold non-authoritative, scores bornés, expiration, digest déterministe | `scenario_evidence.rs` fileciteturn51file0 | Conforme à la règle “evidence propose, optimizer décide”. |
| Statistics | Modules histogram/correlation/ndv/publication ; moteur avec EquiWidth/EquiDepth/Adaptive, thresholds | `statistics/mod.rs` et `engine.rs` fileciteturn47file0 fileciteturn48file0 | Socle stats présent ; publication/usage optimizer à intégrer. |
| SRPL | Lexer/parser/binder/lowering/interpreter/optimizer modules, compiler facade | `andromeda-srpl/src/lib.rs` et `procedure_compiler.rs` fileciteturn23file0 fileciteturn65file0 | Pipeline présent, encore limité à une tranche étroite. |
| Execution | Admission, dispatch, invocation, registry, result stream, retry, services, surface gate, audit ledger, vertical slice | `andromeda-exec/src/lib.rs` fileciteturn24file0 | Bon pont local ; serveur multi-procedure durable à construire. |
| Transaction | State machine, commit log, 2PL/locks, MVCC, GC, savepoints, WAL adapter | `andromeda-tx/src/lib.rs` fileciteturn18file0 | Très avancé en contrats, doit être raccordé à storage/table runtime. |
| Tx state | `Committed` exige `durable_commit_lsn`; rollback durable exigé | `state.rs` fileciteturn31file0 | Invariant C5 implémenté au niveau state machine. |
| Storage | Backup, BTree, buffer pool, disk manager, cold store, file WAL, HADR, heap, manifest, page, recovery, segment, WAL codec | `andromeda-storage/src/lib.rs` fileciteturn19file0 | Large fondation ; runtime durable complet encore à intégrer. |
| File WAL | FileWal on-disk V0, durable prefix, truncated tail, SafeStart/ForensicStart tests | `file_wal.rs` fileciteturn32file0 | V0 recoverable crédible sur mono-segment WAL. |
| Recovery | FastStart/SafeStart/ForensicStart, planning, replay, undo, catalog replay, WAL replay | `recovery.rs` fileciteturn33file0 | Recovery architecture présente, encore à relier aux pages/tables/indexes réels. |
| Pages | PageHeader/PageTrailer/PageImage/PageStore, 16/32 KiB, WAL-before-page-flush | `page.rs` fileciteturn69file0 | Contrats page forts ; stockage disque réel multi-page à poursuivre. |
| Buffer pool | Frames, dirty tracker, pin/unpin, clock eviction, WAL durability observer | `buffer_pool/mod.rs` fileciteturn72file0 | Skeleton sérieux, à intégrer à disk manager et execution runtime. |
| Manifest | DatabaseManifest, storage format manifest, snapshot publication, cold publication placement | `manifest.rs` fileciteturn70file0 | Très proche doctrine snapshot/manifest. |
| Proto | FrameEnvelope, RPC streams, ErrorEnvelope, deterministic serialization, generated protobuf | `andromeda-proto/src/lib.rs` fileciteturn20file0 | Contrat protocolaire local avancé. |
| QUIC | Frame codec, lifecycle, stream roles, backpressure, dispatch, HADR streams, optional Quinn runtime | `andromeda-quic/src/lib.rs` fileciteturn21file0 | Surface runtime en préparation ; réseau réel à valider. |
| Observe | Events, exporters, principal binding, restore trace, trace IDs | `andromeda-observe/src/lib.rs` fileciteturn25file0 | Observability structurée, sink durable à durcir. |
| Bench | Workloads, budgets, evidence, harness WAL/BTree/page/audit/SRPL, regression detection | `andromeda-bench/src/lib.rs` fileciteturn26file0 | Benchmark diagnostique borné déjà présent. |
| CLI | vertical, vertical-v0, protocol-smoke, recovery-inspect, hadr, audit, benchmark, backup, restore, catalog | `cmd.rs` fileciteturn29file0 | Entrée de validation utile, non serveur DB. |
| CLI gates | Tests sans gRPC, sans SQL deps, sans `unsafe` source, vertical-v0/recovery/protocol deterministic | `cli_v0_gates.rs` fileciteturn57file0 | Excellente barrière de dérive doctrinale. |
| HA/DR | Quorum, fencing, membership, promotion, shipping runtime | `hadr.rs` fileciteturn73file0 | Modèle V0 présent ; cluster réel à construire. |
| Backup/PITR | Backup metadata, physical plan, scheduler, checkpoint manager, WAL archive integration, PITR validation | `backup.rs` fileciteturn74file0 | Contrats présents ; exécution production à compléter. |

### 3.2 Ce qui est partiel ou explicitement scaffold

| Élément | État | Risque | Action prioritaire |
|---|---|---|---|
| Optimizer réel | `plan_cache.rs` déclare “SCAFFOLD ONLY” ; pas de sélection de plan ni runtime cache fileciteturn50file0 | Confondre key scaffold avec optimizer | Créer crate/module optimizer minimal avec opérateurs physiques, costing, DecisionTrace. |
| ScenarioEvidence | Scaffold non-authoritative ; pas benchmark runtime intégré au catalog/optimizer fileciteturn51file0 | Evidence non consommée ou consommée trop fortement | Ajouter publication/expiration/consommation contrôlée par optimizer. |
| SRPL DefinitionBatch bridge | Le commit indique bridge/interpreter/lowering/parser, mais un morceau de dry-run SRPL reste TODO dans diff du commit `e0b743` fileciteturn45file0 | Catalogue accepte des procédures non réellement compilées | Implémenter dry-run complet parse → bind → lower → manifest → reject. |
| Runtime réseau | QUIC runtime optional Quinn existe côté crate, mais README parle d’absence serveur réseau complet fileciteturn6file0 | Croire que l’Application Surface est production | Construire serveur local QUIC minimal puis tests mTLS/backpressure. |
| Storage durable complet | FileWal et PageStore existent, mais README exclut full storage engine fileciteturn6file0 | WAL seul sans tables/pages durables | Lier heap/table/page/disk/buffer/checkpoint/recovery. |
| GPU runtime | Policy GPU présente, pas de kernel/runtime GPU | Surinvestissement GPU trop tôt | Reporter GPU runtime après stats CPU/analytics CPU stables. |
| HA/DR production | Modèle quorum/fencing/shipping présent | Split-brain si précipité | Simulateur cluster + WAL shipping local avant réseau réel. |
| CI publique | Aucun statut/workflow exploitable observé dans les appels de statut | Incertitude build réelle | Exécuter localement `cargo fmt`, `cargo check`, `cargo test`, `clippy`. |

---

## 4. Règles de gestion non négociables

### 4.1 Règle C5 — visibilité transactionnelle

**Règle :** une mutation n’est jamais visible avant WAL durable.  
**Implémentation cible :** `TransactionStateMachine::publish_visible_commit_after_durable_flush(lsn)` doit rester l’unique passage vers `Committed`.  
**Test obligatoire :** tentative de `DurableWalFlushed` sans `durable_commit_lsn` doit échouer ; crash après WAL flush mais avant dirty page doit rejouer la mutation ; crash avant WAL flush ne doit pas rendre visible la mutation.

### 4.2 Règle Procedure-only

**Règle :** aucune surface applicative ne reçoit SQL ad hoc, query text, statement text, raw SQL ou dépendance client SQL.  
**Implémentation cible :** continuer les scans de `cli_v0_gates.rs`, mais les déplacer aussi en test workspace global et hook CI.  
**Critère :** toute nouvelle crate ou dépendance qui introduit `sqlx`, `tokio-postgres`, `rusqlite`, `tonic`, `grpc`, `serde_json` runtime doit être refusée sauf décision explicite hors runtime.

### 4.3 Règle contractuelle

**Règle :** toute invocation doit lier `ProcedureId + ContractHash + CatalogVersion + StatsVersion + PolicyVersion`.  
**Implémentation cible :** `ProcedureContractBinding` devient l’entrée obligatoire pour execution, plan cache, audit, benchmark evidence et protocol manifest.  
**Critère :** aucune invocation ne crée de `TransactionScope` avant validation contrat + IAM + surface.

### 4.4 Règle GPU

**Règle :** GPU interdit pour `Commit`, `WalAppend`, `Rollback`, `Recovery`, sécurité critique, MVCC courte et lookup ligne par ligne.  
**Autorisé :** histograms, NDV approximatif validé, skew detection, scans analytiques, agrégations massives, benchmark scenarios, vectoriel/embedding en extension contrôlée.  
**Critère :** tout module GPU doit appeler `GpuProfile::validate_pipeline(PipelineClass)` et produire une trace d’usage hardware.

### 4.5 Règle evidence

**Règle :** Procedure Store observe, Benchmark produit evidence, ScenarioEvidence conseille, Optimizer décide.  
**Interdit :** benchmark ou evidence qui force un plan sans DecisionTrace, validation, fallback et expiration.  
**Critère :** `ScenarioEvidence::is_authoritative()` doit rester `false`.

### 4.6 Règle Storage hot/cold

**Règle :** RAM/BufferPool ne sont pas vérité ; NVMe est chaud/récent ; HDD est vérité froide via snapshots publiés ; ColdStore est immutable/append-only ; GPU n’est jamais vérité.  
**Critère :** aucune page froide publiée ne subit update-in-place ; toute mutation produit nouvelle version/segment/manifest.

### 4.7 Règle d’observabilité

**Règle :** toute décision importante doit avoir trace.  
**Minimum :** `ProcedureInvocationTrace`, `TransactionTrace`, `PlanDecisionTrace`, `CatalogChangeTrace`, `DefinitionBatchApplyTrace`, `SecurityAuditTrace`, `RecoveryTrace`, `ClusterEventTrace`, `BenchmarkEvidenceTrace`, `GpuExecutionTrace`.  
**Critère :** fonctionnalité non traçable = refus du noyau.

---

## 5. Architecture cible et mapping crates

| Moteur cible | Crates actuelles | Statut | Prochaine consolidation |
|---|---|---|---|
| Core Engine | `andromeda-core` | Bon socle | Ajouter hardware discovery réel, resource accounting global. |
| Catalog & Contract Engine | `andromeda-catalog` | Avancé | Store durable + DefinitionBatch réel + dépendances + publication WAL. |
| SRPL Compiler | `andromeda-srpl` | Pipeline étroit | Grammaire V0 complète + binder catalogue + diagnostics. |
| Execution Engine | `andromeda-exec` | Vertical local | Dispatcher multi-procedure + transaction lifecycle intégré + result streaming. |
| Transaction Kernel | `andromeda-tx` | Avancé en contrats | Intégration table/page/index + isolation policies effectives. |
| Storage Engine | `andromeda-storage` | Très large fondation | Disk manager + heap + buffer pool + checkpoint + manifest switch E2E. |
| Protocol/RPC | `andromeda-proto`, `andromeda-quic` | Contrats avancés | Serveur QUIC local, mTLS, flow/backpressure réel. |
| Security/IAM Plane | `andromeda-core`, `andromeda-exec`, `andromeda-quic`, `observe` | Partiel | UserPrincipal registry durable + policies + audit immuable. |
| Observability/Forensic | `andromeda-observe` | Partiel | Durable audit ledger + query trace + retention. |
| Statistics/Optimizer | `andromeda-catalog`, futur `andromeda-optimizer` possible | Stats scaffold avancé, optimizer non réel | Cost model + operators + plan decision trace. |
| Benchmark Engine | `andromeda-bench`, `andromeda-cli` | Diagnostique borné | Bascule vers ScenarioEvidence publiée et régression. |
| Analytics/GPU | `andromeda-core` policy + futur runtime | Policy seulement | Runtime CPU columnar puis GPU optionnel. |
| HA/DR/Backup | `andromeda-storage`, `andromeda-cli`, `andromeda-quic` | Contrats présents | Simulateur cluster + WAL shipping + PITR E2E. |

---

## 6. Roadmap priorisée

### Phase A — Stabilisation immédiate du `main` et preuve build

**Objectif :** transformer le `main` en base vérifiable localement avant d’ajouter de nouvelles ambitions.

**Tâches :**

1. Exécuter et corriger :
   - `cargo fmt --all -- --check`
   - `cargo check --workspace`
   - `cargo test --workspace`
   - `cargo clippy --workspace --all-targets`
2. Si `andromeda-storage` contient encore des erreurs préexistantes mentionnées par le commit `e0b743`, créer une issue interne ou DEC courte par erreur.
3. Ajouter un `docs/ROADMAP_IMPLEMENTATION_2026.md` issu de ce document.
4. Ajouter un `docs/CURRENT_STATE.md` court : “ce qui est runtime”, “ce qui est scaffold”, “ce qui est doctrine”.
5. Corriger README : retirer ou actualiser les références Wave 14 non présentes si elles ont été supprimées au nettoyage.

**Critères d’acceptation :**

- Les quatre commandes passent localement.
- Le README ne référence pas de documents absents.
- Les tests `cli_v0_gates.rs` restent passants.
- Aucune nouvelle dépendance SQL/gRPC/runtime JSON.

---

### Phase B — Catalog durable + DefinitionBatch réel

**Objectif :** faire du catalogue la vérité contractuelle durable, pas seulement une fixture ou un modèle mémoire.

**Tâches principales :**

1. Implémenter `CatalogStore` durable V0 :
   - tables système cataloguées ;
   - `CatalogVersion` monotone ;
   - `PolicyVersion` et `StatsVersion` reliées ;
   - WAL records catalog changes ;
   - snapshot catalog minimal.
2. Implémenter `DefinitionBatch::dry_run` complet :
   - parse opérations ;
   - canonicalize ;
   - dependency graph ;
   - validate names/types/contracts ;
   - validate SRPL procedures ;
   - compute impact : locks, recompilation, storage, security.
3. Implémenter `ApplyDefinitionBatch` transactionnel :
   - begin catalog transaction ;
   - write WAL ;
   - publish `CatalogVersion` atomique ;
   - emit `DefinitionBatchApplyTrace` ;
   - rollback complet en cas d’échec.
4. Formaliser compatibilité contrat :
   - additive ;
   - breaking ;
   - deprecated ;
   - rejected ;
   - exact hash.
5. Brancher `ProcedureContractBinding` partout : execution, protocol, plan cache, audit.

**Sous-tâches test :**

- Dry-run rejette procédure SRPL invalide.
- Dry-run rejette changement breaking non autorisé.
- Apply crash au milieu ne publie pas demi-catalogue.
- Recovery reconstruit le dernier catalogue publié.
- ContractHash mismatch est rejeté avant transaction.

**Critères d’acceptation :**

- Toute procedure visible dans le catalog a `ContractHash`, `CatalogVersion`, `StatsVersion`, `PolicyVersion` non zéro.
- Une invocation ne peut jamais référencer une procédure hors version publiée.

---

### Phase C — SRPL V0 normatif et intégration IR

**Objectif :** passer d’une tranche SRPL étroite à un langage V0 exploitable pour procédures transactionnelles simples.

**Syntaxe V0 minimale :**

- `procedure` / `transaction procedure`
- `accepts` / `takes`
- `returns`
- `reads` / `writes`
- `with isolation/access/semantics/missing values`
- `ensure ... else fail`
- `let ... from ... where ...`
- `insert`, `update`, `delete`
- `return ... from ...`
- `for each` collection finie
- agrégats avec `else`
- `optional one` + `when has value otherwise`

**Tâches :**

1. Stabiliser la grammaire V0.
2. Ajouter binder catalogue : tables, columns, types, enums, structured objects, maps.
3. Implémenter cardinality checker : `one`, `optional one`, `many`, `nonempty many`.
4. Refuser : `while`, `goto`, SQL dynamique, réseau/fichier, random non seedé, `SELECT *`, noms positionnels.
5. Lowering vers IR relationnel typé : scan/filter/project/join/group/mutate/return.
6. Diagnostics SRPL : cardinalité non garantie, absence non traitée, mutation trop large, float interdit, ordre non contractuel.
7. Intégrer SRPL au DefinitionBatch dry-run.

**Critères d’acceptation :**

- `Inventory.ReserveStock` peut être exprimée en SRPL V0 et exécutée via IR, non uniquement via handler métier Rust.
- Les erreurs SRPL retournent diagnostics source spans.
- Le même SRPL produit le même `ContractHash` et le même IR canonique.

---

### Phase D — Storage Engine durable V0

**Objectif :** lier WAL fichier, pages, heap, buffer pool, disk manager, checkpoint et recovery dans un chemin durable minimal.

**Tâches :**

1. `DiskPageStore` réel :
   - read/write page ;
   - page size canonique ;
   - fsync/fdatasync policy ;
   - torn write detection ;
   - page trailer verification.
2. Heap table V0 :
   - row encoder ;
   - slot directory ;
   - insert/update/delete ;
   - row versions MVCC.
3. BufferPool intégré :
   - fetch page from DiskPageStore ;
   - pin/unpin via guards ;
   - dirty tracking ;
   - flush only when WAL durable.
4. Checkpoint V0 :
   - checkpoint begin/end records ;
   - dirty pages eligible ;
   - recovery window update ;
   - WAL truncation gated.
5. Cold snapshot V0 :
   - staging segment ;
   - validate ;
   - manifest switch atomique ;
   - previous manifest fallback.
6. Recovery E2E :
   - manifest → snapshot → WAL replay ;
   - redo committed ;
   - skip rolled back/incomplete ;
   - rebuild heap/index skeleton ;
   - `RecoveryReport` durable.

**Tests obligatoires :**

- Crash avant WAL flush.
- Crash après WAL flush avant page flush.
- WAL record tronqué.
- Manifest corrompu.
- Snapshot tmp incomplet.
- Torn write page.
- Dirty page with durable LSN behind page LSN rejected.
- Catalog batch crash.

**Critères d’acceptation :**

- `vertical-v0` ne se contente plus d’un WAL payload métier ; il modifie une table heap durable.
- `recovery-inspect` peut reconstruire l’état table attendu.

---

### Phase E — Transaction Kernel + MVCC/Isolation effectifs

**Objectif :** rendre les garanties transactionnelles testables sur données persistantes, pas seulement sur state machine.

**Tâches :**

1. Connecter `TransactionManager` au heap/table/page runtime.
2. Implémenter snapshot acquisition/release via `ActiveSnapshotRegistry`.
3. Intégrer MVCC row visibility : `BeginTs`, `EndTs`, creator/deleter Tx.
4. Implémenter write set et rollback avec savepoints.
5. 2PL strict minimal pour writes critiques.
6. Définir isolation V0 :
   - `ReadCommitted` optionnel ;
   - `Snapshot` ;
   - `Serializable` via 2PL strict ou SSI scaffold plus tard.
7. GC MVCC : oldest active snapshot, backup pin, replica lag, forensic retention.
8. Deadlock/timeout/retry semantics.

**Critères d’acceptation :**

- Write skew est détecté/refusé pour procédures déclarées serializable.
- Long reader bloque GC mais déclenche metrics/quota.
- Rollback restaure table + indexes + maps immédiates.

---

### Phase F — Execution Engine multi-procedure

**Objectif :** remplacer la seule tranche verticale par un dispatch generic de procedures cataloguées.

**Tâches :**

1. `ProcedureRegistry` devient catalogue-backed.
2. `ProcedureDispatcher` reçoit `ProcedureName + ContractHash + payload`.
3. Admission pipeline :
   - protocol validate ;
   - contract validate ;
   - IAM validate ;
   - resource budget ;
   - transaction creation.
4. SRPL execution adapter : IR → physical operators minimal.
5. ResultStream : metadata before payload, row count exact policy, batch sizing, backpressure.
6. Error routing : business/system/contract/transaction/resource.
7. Procedure Store records : duration, rows read/written, wal bytes, temp bytes, plan id, error kind.

**Critères d’acceptation :**

- Deux procédures cataloguées peuvent être appelées sans handler spécial codé en dur.
- Contract mismatch ne crée aucune transaction.
- Permission denied ne crée aucune transaction.
- Result metadata est toujours émise avant payload.

---

### Phase G — RPC/QUIC runtime local

**Objectif :** passer de protocol-smoke local à serveur/client QUIC minimal.

**Tâches :**

1. Activer `runtime-quinn` derrière feature et profil test.
2. Implémenter `Application Surface` locale : HELLO, AUTH, CONTRACT_REQUEST, RPC_EXECUTE_REQUEST, RPC_METADATA/BATCH/COMPLETION, ERROR.
3. mTLS dev certificates avec `rcgen` uniquement pour tests locaux.
4. Surface separation : Application/Admin/Cluster.
5. Stream role validation : application ne peut pas faire admin/cluster.
6. Backpressure : slow client, buffer cap, batch reduce, reject/shed.
7. Zero-RTT policy : refus par défaut pour mutations, admission stricte pour idempotent read-only.
8. Protocol fuzz/property tests : frame length, header CRC, payload kind lockstep, malformed frames.

**Critères d’acceptation :**

- Client test appelle `Inventory.ReserveStock` via QUIC local.
- Aucune socket n’est ouverte dans `protocol-smoke`; tests réseau séparés.
- Admin frame sur Application Surface est rejetée avec audit.

---

### Phase H — Security/IAM/Admin/Audit

**Objectif :** rendre la sécurité réelle, durable et auditée.

**Tâches :**

1. System Database registries : UserPrincipal, CertificateIdentity, Roles, Permissions, Policies.
2. IAM pipeline : cert → certificate identity → user principal → roles/groups/direct permissions → policies → audit.
3. Permissions minimales : ExecuteProcedure, ReadContract, Create/Alter/Drop objects, ImportDefinitionBatch, DebugProcedure, Backup, Restore, ClusterPromote, AuditRead.
4. Policies : surface, resource, time, break-glass, data/tenant, namespace.
5. Security Maps V0 pour row/tenant constraints.
6. Audit ledger durable : append-only, signed/hashed chain, retention policy, query admin.
7. Admin Surface : catalog import, backup/restore, debug snapshot, plan inspect, stats publish.

**Critères d’acceptation :**

- Certificat applicatif ne peut pas utiliser Admin Surface.
- User disabled refuse toute invocation.
- Break-glass est borné, tracé et expire.
- Audit cannot be globally disabled.

---

### Phase I — Optimizer minimal réel

**Objectif :** créer un optimiseur cost-based minimal, traçable, sans learned components au départ.

**Tâches :**

1. Définir `ProcedurePlan` runtime : operators, read/write sets, budgets.
2. Operators V0 : TableScan, IndexSeek, Filter, Projection, NestedLoopJoin, HashJoin, AggregateHash/Stream, Sort, DmlUpdate, ResultStreamBuild.
3. Cost model V0 : CPU + logical IO + physical IO + WAL + temp + network + risk penalty.
4. Stats consumption : row count, histograms, NDV, skew, column stats.
5. Plan enumeration bornée : top-k, no combinatorial blowup.
6. DecisionTrace : candidates considered/rejected, costs, cardinalities, stats, Procedure Store feedback, evidence used/ignored.
7. Runtime Plan Cache : key strictement `PlanCacheKey`, quota, eviction, no silent reuse.
8. Hysteresis anti-flapping.

**Critères d’acceptation :**

- Même contrat/catalog/stats/policy/shape produit même plan key.
- Changement de StatsVersion invalide le plan.
- Plan mauvais peut être expliqué par DecisionTrace.

---

### Phase J — Statistics Engine publication

**Objectif :** rendre les statistiques publiables, versionnées, validées et consommées.

**Tâches :**

1. Stats collection CPU : table row count, histograms, NDV, null count, density.
2. Histogram algorithms : equi-width, equi-depth, adaptive.
3. Correlation approximations multi-colonnes limitées.
4. Stats invalidation : mutation %, max age, LSN delta.
5. Candidate StatsVersion : collect → validate → publish controlled switch.
6. Publication rollback : if bad stats cause regressions, revert StatsVersion.
7. Procedure Store feedback : actual rows vs estimated rows.
8. Skew detection.

**Critères d’acceptation :**

- Stats active ne change jamais silencieusement.
- Every StatsVersion has validation trace.
- Optimizer can explain stats used.

---

### Phase K — Benchmark Engine + Predictive Evidence

**Objectif :** transformer le benchmark diagnostique en moteur d’evidence borné, sans le rendre décisionnaire.

**Tâches :**

1. Étendre workloads : vertical, protocol, WAL append, WAL replay, BTree lookup/range, page store, audit sink, SRPL compile, stats build, map refresh.
2. Ajouter GPU benchmark **diagnostique uniquement** après GPU runtime.
3. Ajouter `BenchmarkHistoryStore` durable.
4. Générer `ScenarioEvidence` depuis benchmark validé : score, confidence, validity window, target ProcedureId/StatsVersion/ContractHash/PlanClass.
5. Regression detection : baseline, budget thresholds, reasons.
6. Resource budgets : max duration/samples/warmups/temp bytes/GPU time.
7. Integrate with optimizer as advisory input only.
8. Ajouter poisoning defense : limiter influence de l’historique, isoler workloads synthétiques, invalider evidence anormale.

**Critères d’acceptation :**

- Benchmark ne peut pas forcer un plan.
- Toute evidence expire.
- Evidence ciblée sur StatsVersion N ne s’applique pas à StatsVersion N+1 sans republication.

---

### Phase L — Maps analytiques et moteur analytique

**Objectif :** matérialiser les Maps selon grain, cohérence, storage layout et statistiques.

**Tâches :**

1. `MapDescriptor` catalogue : definition IR, grain, refresh policy, storage layout, staleness, stats.
2. Immediate Map simple : même transaction, WAL table + map.
3. Incremental Map : delta log + apply controlled + validate + publish version.
4. Deferred/SnapshotOnly Map : job admin, snapshot isolation, columnar storage.
5. Analytical Map columnar : column segments, compression, dictionary, min/max stats.
6. Summarizability constraints : grain déclaré, agrégats compatibles, dimensions stables.
7. Map optimizer use : MapLookup operator only if staleness/policy allows.
8. Map maintenance budgets : cannot starve WAL/checkpoint.

**Critères d’acceptation :**

- Immediate Map update est atomique avec table source.
- Aggregate Map ne rentre pas en Immediate sans coût borné.
- Analytical Map large ne touche pas commit path OLTP.

---

### Phase M — GPU Runtime hors commit path

**Objectif :** ajouter un accélérateur GPU strictement batch, optionnel, testable, désactivable.

**Précondition :** Phase J stats CPU stable + Phase L analytical maps CPU stable.

**Tâches :**

1. Créer module/crate optionnel `andromeda-gpu` ou sous-module hardware runtime.
2. Backend abstrait : `GpuDevice`, `GpuKernel`, `GpuBatch`, `GpuExecutionTrace`, `GpuFallbackReason`.
3. Device discovery : vendor, memory, compute capability, driver version, queue limits.
4. Policy gate obligatoire : `GpuProfile::validate_pipeline`.
5. Pipelines autorisés V1 :
   - `StatisticsRefresh` : histograms/NDV/skew ;
   - `MapRefresh` : columnar aggregate batch ;
   - `BatchAnalytics` : scan/aggregate/vector.
6. Fallback CPU obligatoire.
7. Validation CPU : sample-based or full deterministic validation depending criticality.
8. GPU memory budget : pinned host memory, device memory, batch size, transfer bytes.
9. No GPU for WAL, commit, rollback, recovery, security, point lookup.
10. Bench GPU : diagnostic only, no plan forcing.
11. Observability : duration, transfer bytes, kernel id, fallback reason, validation status.
12. Kill switch global + per database + per procedure/map.

**Critères d’acceptation :**

- Désactiver GPU ne change jamais le résultat contractuel.
- GPU failure routes to CPU fallback or controlled ResourceError.
- Recovery works even if GPU disappeared.
- No WAL/commit path imports `andromeda-gpu`.

---

### Phase N — HA/DR, backup, PITR, forensic

**Objectif :** rendre HA/DR et backup testables en simulation, puis en runtime.

**Tâches :**

1. WAL archive manager : segment archive, hash, retention.
2. Backup plan execution : cold snapshot + WAL archives + catalog + audit + manifests.
3. PITR restore : choose snapshot, verify manifest, replay WAL to LSN/timestamp, validate invariants.
4. HA simulator : primary, replica, witness, lag, fencing, promotion.
5. WAL shipping local : sync/async modes, LSN tracking.
6. Quorum/fencing : no self-promotion, split-brain simulation.
7. ForensicStart : block app connections, produce coherence report.
8. Restore drills as tests.

**Critères d’acceptation :**

- Replica lag prevents promotion if policy says so.
- Backup restore is tested, not assumed.
- ForensicStart never mutates truth through redo.

---

### Phase O — Enterprise Grade hardening

**Objectif :** passer de prototype riche à système exploitable en environnement sérieux.

**Tâches :**

1. Formaliser release gates par phase.
2. Fuzz/property tests : WAL codec, page codec, frame codec, SRPL parser, catalog batches.
3. Crash test matrix automatisée.
4. Performance budgets par workload.
5. Security review : mTLS, cert rotation, audit immutability, break-glass.
6. Observability dashboards local CLI.
7. Documentation normative courte par spec : ProcedureContract, TypeSystem, WalRecord, PageHeader, Manifest, FrameHeader, TransactionStateMachine.
8. API client SDK typé minimal.
9. Migration compatibility policy.
10. Red team prompt/agent artifacts only if AI operating layer is kept in repo.

---

## 7. Backlog P0/P1/P2

### P0 — Ne pas continuer sans ça

| Item | Crate/module | Livrable | Critère |
|---|---|---|---|
| Vérification build workspace | root | rapport `cargo` | fmt/check/test/clippy pass. |
| README/doc drift cleanup | docs | README cohérent | pas de lien Wave absent. |
| DefinitionBatch dry-run SRPL réel | catalog/srpl | parse-bind-lower-manifest | rejet invalide all-or-nothing. |
| Catalog durable minimal | catalog/storage | CatalogStore + WAL records | recovery catalogue publié. |
| Heap durable `Inventory.ProductStock` | storage/exec | table durable | vertical-v0 modifie table. |
| Recovery table state | storage | recovery E2E | état stock reconstruit après crash. |
| ProcedureContractBinding everywhere | catalog/exec/proto | binding complet | no zero stats/policy. |

### P1 — Ensuite seulement

| Item | Crate/module | Livrable | Critère |
|---|---|---|---|
| QUIC app surface local | quic/proto/exec | client/server test | call procedure over QUIC. |
| IAM durable | core/catalog/observe | principal/cert registry | cert app != admin. |
| Optimizer minimal | catalog/exec/futur optimizer | plan + DecisionTrace | deterministic plan key. |
| Statistics publication | catalog | StatsVersion publish/switch | active stats versioned. |
| Procedure Store runtime | catalog/exec | invocation records | actual vs estimated rows. |
| BufferPool integrated | storage | page fetch/flush | WAL-before-page-flush. |
| BTree access path V0 | storage | seek/range | recovered or rebuilt. |

### P2 — Accélération et sophistication

| Item | Crate/module | Livrable | Critère |
|---|---|---|---|
| Maps immediate/incremental | catalog/storage/exec | MapDescriptor + delta | atomic or controlled. |
| Benchmark evidence publication | bench/catalog | ScenarioEvidence from run | non-authoritative, expirable. |
| GPU stats batch | gpu/catalog/bench | GPU histograms | CPU validation/fallback. |
| Analytical column segments | storage/analytics | columnar Map | no commit path. |
| HA simulator | storage/quic | quorum/fencing tests | no split-brain. |
| PITR E2E | storage/cli | restore test | target LSN exact. |

---

## 8. Décisions ouvertes à trancher

| Décision | Options | Recommandation |
|---|---|---|
| Crate optimizer dédiée ? | `andromeda-catalog::optimizer` ou `andromeda-optimizer` | Crate dédiée si le volume grossit ; garder plan_cache types dans catalog. |
| Crate gpu dédiée ? | `andromeda-gpu` ou core module | Crate optionnelle dédiée pour éviter import accidentel dans commit path. |
| Page size défaut | 16 KiB ou 32 KiB | 16 KiB pour OLTP V0, 32 KiB pour cold/analytics à policy. |
| Endianness | little-endian canonique | Choisir explicitement little-endian ; tests cross-arch. |
| Decimal internal | scaled integer / BCD / custom | scaled integer V0, BCD/custom plus tard. |
| Isolation V0 critique | 2PL strict / SSI / deterministic | 2PL strict pour critique V0, SSI plus tard. |
| CatalogVersion granularity | par DB / instance / hybride | par DB + system global pour registries. |
| GPU backend | CUDA / ROCm / wgpu / abstraction | abstraction d’abord, backend unique expérimental ensuite. |
| Learned CE | expérimental / advisory / production | advisory C0/C1, jamais C5 seul. |
| QUIC runtime | feature-gated / always-on | feature-gated jusqu’aux tests sécurité. |

---

## 9. Ordre recommandé des 30 prochaines tâches

1. Corriger/valider `cargo check --workspace`.
2. Corriger/valider `cargo test --workspace`.
3. Nettoyer README des références Wave absentes ou restaurer les fichiers manquants si souhaité.
4. Ajouter ce document en `docs/ROADMAP_IMPLEMENTATION_2026.md`.
5. Écrire `docs/CURRENT_STATE.md` avec runtime/scaffold/doctrine.
6. Implémenter SRPL dry-run réel dans DefinitionBatch.
7. Ajouter tests dry-run invalid procedure all-or-nothing.
8. Implémenter CatalogStore durable minimal.
9. Ajouter WAL catalog change replay.
10. Connecter `ProcedureContractBinding` à execution path.
11. Convertir `Inventory.ReserveStock` vers SRPL source canonique et IR.
12. Implémenter heap durable minimal pour `ProductStock`.
13. Brancher BufferPool + PageStore sur heap insert/update.
14. Faire `vertical-v0` modifier heap/table durable.
15. Faire recovery reconstruire `ProductStock`.
16. Ajouter crash tests vertical-v0 table durable.
17. Ajouter Procedure Store invocation record runtime.
18. Publier `StatsVersion` basique pour table durable.
19. Ajouter optimizer minimal single-plan avec DecisionTrace.
20. Ajouter PlanCache runtime borné sur `PlanCacheKey`.
21. Ajouter QUIC application server local test.
22. Ajouter mTLS dev identity + surface scope.
23. Ajouter IAM principal registry durable minimal.
24. Ajouter audit durable append-only sink.
25. Ajouter Map immediate simple sur table source.
26. Ajouter benchmark storage/WAL/procedure regression baseline.
27. Publier ScenarioEvidence depuis benchmark, non-authoritative.
28. Ajouter HA simulator local avec quorum/fencing.
29. Ajouter PITR restore E2E sur WAL + snapshot.
30. Préparer GPU runtime design DEC sans implémenter kernels avant CPU stats/maps stables.

---

## 10. Synthèse finale

Andromeda doit continuer en gardant une hiérarchie stricte :

```text
1. Vérité et recovery.
2. Catalogue et contrats.
3. SRPL et execution transactionnelle.
4. RPC sécurisé.
5. Statistiques et optimizer.
6. Maps analytiques.
7. Benchmark evidence.
8. GPU hors commit path.
9. HA/DR/backup production.
```

La tentation naturelle sera d’aller vite vers GPU, optimizer learned ou analytics. Ce serait une erreur d’ordre. Les sources et le dépôt convergent : **le GPU est déjà correctement cadré comme accélérateur batch, pas comme composant transactionnel**. Le prochain gain réel n’est pas GPU ; c’est de rendre le chemin `Procedure -> Catalog -> SRPL IR -> Transaction -> WAL -> Page/Table durable -> Recovery -> ResultStream` entièrement réel, minimal, testable et récupérable.

Une fois ce vertical durable stabilisé, le GPU aura une place nette : statistiques, Map refresh, scans analytiques, benchmark scenario, vectoriel contrôlé. Avant cela, il doit rester une policy et un futur module, pas une dépendance runtime.
