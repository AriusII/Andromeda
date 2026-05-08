# 05 — Optimizer, Statistics, Analytics, Hardware, Roadmap et Sources

> Consolidation : **Andromeda — SGBDRT Moderne 2026**  
> Date : **2026-05-06**  
> Nature : **Markdown final consolidé**  
> Sources intégrées : 18 fichiers Markdown V0, 1 complément Storage Engine, 3 PDF de fondation.  
> Doctrine : relationnel transactionnel, RPC-only, SRPL, typage fort, déterminisme, observabilité, recovery, Enterprise Grade.


## 1. Périmètre du document

Ce document consolide la partie adaptative et stratégique du projet : Procedure Store, Optimizer, Statistics Engine, Predictive Evidence, Maps analytiques, hardware CPU/GPU/NVMe/HDD, Rust, roadmap V0, risques, décisions ouvertes et corpus scientifique.

La règle centrale : **l’adaptation est autorisée uniquement si elle est bornée, versionnée, observable, explicable et désactivable**. Andromeda doit être performant, mais pas au prix d’une boîte noire incontrôlable.

## 2. Procedure Store

Procedure Store remplace conceptuellement Query Store. Il ne mémorise pas des requêtes SQL libres ; il mémorise des invocations réelles de Procedures cataloguées.

### 2.1 Champs minimaux

```text
InvocationId
ProcedureId
ContractHash
CatalogVersion
StatsVersion
PlanId
PlanClass
InputShapeHash
StructuredObjectShapeHash
RowsRead
RowsReturned
RowsWritten
RowsAffected
Duration
LogicalReads
PhysicalReads
CacheHits
WalBytes
TempBytes
SpillBytes
ErrorKind
DecisionTraceId
ResourceBudget
```

### 2.2 Usages

| Usage | Description |
|---|---|
| Diagnostic | Comprendre lenteurs, erreurs, spills, plans instables. |
| Optimizer feedback | Alimenter costing et choix multi-plan. |
| Capacity planning | Identifier CPU/RAM/NVMe/WAL/temp pressure. |
| Security/audit | Corréler exécution, principal, permissions, contrat. |
| Regression detection | Détecter plan plus lent après stats/catalog change. |
| Benchmark control | Comparer scenario evidence et réel observé. |

Procedure Store observe ce qui s’est passé. Il ne doit pas devenir seul décideur.

## 3. Optimizer

L’Optimizer décide du plan. Il consomme :

```text
Catalog
Constraints
StatsVersion
Procedure Store
ScenarioEvidence
HardwareProfile
StorageTemperature
PolicyVersion
ContractHash
InputShape
StructuredObjectShape
ResourceBudget
```

Positionnement :

```text
cost-based + evidence-based + workload-aware + stability-aware + bounded
```

### 3.1 ProcedurePlan

```text
ProcedurePlan {
    PlanId,
    ProcedureId,
    ContractHash,
    CatalogVersion,
    StatsVersion,
    PlanClass,
    PhysicalOperators,
    ResourceBudget,
    DecisionTraceId
}
```

### 3.2 DecisionTrace

Une décision de plan doit pouvoir expliquer :

```text
candidats considérés
candidats rejetés
coûts estimés
cardinalités estimées
stats utilisées
histogrammes/skew pertinents
feedback Procedure Store utilisé
ScenarioEvidence utilisé ou ignoré
policies applicables
choix final
raison de stabilité/hysteresis si plan conservé
```

## 4. Multi-plan borné

Andromeda peut conserver plusieurs plans pour une même Procedure, mais uniquement sous classes bornées.

| PlanClass | Usage |
|---|---|
| Generic | Cas général. |
| Small | Entrées petites, faible cardinalité. |
| Medium | Entrées moyennes. |
| Large | Entrées grandes. |
| Skewed | Distribution déséquilibrée. |
| StructuredObjectSmall | Paramètre tabulaire petit. |
| StructuredObjectLarge | Paramètre tabulaire grand. |
| Maintenance | Procédures admin/maintenance. |

Garde-fous :

```text
quota de plans par Procedure
quota global de plans
fusion des plans dominés
éviction par usage/stabilité
hysteresis anti-flapping
validation après CatalogVersion/StatsVersion change
```

## 5. Statistics Engine

Le Statistics Engine produit :

```text
cardinality
distribution
histograms
skew
density
selectivity
correlation approximations
storage temperature
row count exact at snapshot si disponible
```

Pipeline :

```text
Collect CPU
  -> batch GPU optionnel
  -> compute
  -> validate CPU
  -> StatsVersion candidate
  -> publish controlled switch
```

Le GPU peut accélérer les calculs statistiques, mais le résultat publié doit être validé et versionné. Les statistiques actives ne changent pas silencieusement.

## 6. Histogrammes et cardinalité

Les histogrammes restent un socle robuste pour l’estimation. Le corpus scientifique rappelle que les erreurs de cardinalité restent une des principales causes des mauvais plans. Andromeda doit donc traiter les statistiques comme un composant de premier rang.

| Problème | Réponse classique | Réponse Andromeda |
|---|---|---|
| Distribution non uniforme | Histogrammes. | Histogrammes + skew score. |
| Corrélations attributs | Stats multi-colonnes partielles. | StatsVersion avec qualité/coverage. |
| Paramètres tabulaires | Guess fragile. | StructuredObjectShapeHash + RowCountExact. |
| Drift | Refresh stats. | Candidate validation + controlled switch. |
| Plan instability | Recompile fréquent. | Hysteresis + multi-plan borné. |

## 7. Predictive Evidence Engine

Le moteur prédictif ne décide pas. Il produit de l’evidence.

```text
hypothèse
scénario
benchmark/simulation
ScenarioEvidence
validation analytique
consommation par Optimizer
expiration
```

### 7.1 ScenarioEvidence

```text
ScenarioEvidence {
    ScenarioId,
    ScenarioHash,
    ProcedureId,
    ObjectId,
    ContractHash,
    CatalogVersion,
    StatsVersion,
    WorkloadIntent,
    ParameterShapeHash,
    StructuredObjectShapeHash,
    EstimatedCardinality,
    EstimatedLogicalCost,
    EstimatedIoTierImpact,
    EstimatedSpillRisk,
    ConfidenceScore,
    CriticalityScore,
    FreshnessScore,
    RealismScore,
    StabilityScore,
    ValidationState
}
```

### 7.2 Scores séparés

Ne jamais fusionner naïvement tous les scores en une pseudo-vérité. Un scénario peut être confiant mais peu réaliste, récent mais peu stable, critique mais peu frais.

| Score | Question |
|---|---|
| ConfidenceScore | Le modèle est-il sûr de son estimation ? |
| CriticalityScore | Le scénario touche-t-il un chemin critique ? |
| FreshnessScore | Les données/scénarios sont-ils récents ? |
| RealismScore | Le scénario ressemble-t-il au workload réel ? |
| StabilityScore | Le résultat est-il stable sous variations ? |

## 8. Garde-fous optimizer/evidence

| Risque | Garde-fou |
|---|---|
| Explosion combinatoire | Top-k, pruning, dominance. |
| Pollution statistique | Validation analytique, échantillons contrôlés. |
| Instabilité des plans | Hysteresis, seuils de gain, rollback plan. |
| Surcharge NVMe | Quotas temp/benchmark, priorités I/O. |
| Surconfiance ML | Scores séparés, fallback classique. |
| Drift workload | Expiration evidence, Procedure Store feedback. |
| Plan forcé dangereux | Policy, audit, mode emergency only. |

## 9. Learned components : position prudente

Les PDF de fondation et de corpus distinguent clairement le consensus durable du front de recherche. Les learned cardinality estimators, learned indexes et learned optimizers sont sérieux, mais ne remplacent pas universellement les histogrammes, B+Trees et optimiseurs à coût.

Position Andromeda :

| Composant appris | Statut recommandé |
|---|---|
| Learned Cardinality Estimation | Expérimental C0/C1 puis C2 si validé ; jamais seul en C5. |
| Learned Index | Option analytique/expérimentale ; fallback B+Tree obligatoire. |
| Learned Optimizer | Evidence ou suggestion ; décision finale gouvernée. |
| Workload prediction | Utile pour capacity planning et scenarios ; pas vérité transactionnelle. |
| Poisoning defense | Obligatoire avant usage basé sur historique. |

Règle : un composant learned peut proposer, jamais contourner les invariants.

## 10. Analytics et Maps

Les Maps analytiques doivent être conçues autour du grain, de la summarizability et de la cohérence.

| Concept | Exigence |
|---|---|
| Grain | Déclarer ce qu’une ligne représente exactement. |
| Summarizability | Garantir que les agrégations restent valides. |
| Refresh mode | Immediate, Incremental, Deferred, SnapshotOnly. |
| Storage layout | Row/column/hybrid selon usage. |
| Stats | Histograms, cardinality, skew, compression stats. |
| GPU | Batch hors commit path seulement. |

Une Map analytique large ne doit pas ralentir le commit OLTP. Elle doit utiliser deferred/snapshot/incremental selon coût.

## 11. Hardware : principes

Stack hardware cible :

```text
CPU x64/ARM64
GPU analytics/statistics/vectoriel
RAM
Cache chaud NVMe/SSD
Vérité saine en support HDD via snapshots
```

La valeur ne vient pas du matériel seul. Elle vient du fait que chaque usage matériel est borné par une policy et relié à une fonction claire.

## 12. Rust

Recommandation V0 : Rust comme langage principal.

Pourquoi Rust :

| Besoin moteur | Apport Rust |
|---|---|
| Mémoire sûre | Ownership, lifetimes, types. |
| Concurrence | Fearless concurrency, atomics explicites. |
| Performance | Contrôle bas niveau sans GC global. |
| FFI | Appels système, GPU, intrinsics. |
| Encapsulation unsafe | API safe autour de zones critiques. |

### 12.1 Unsafe Rust

Unsafe est inévitable pour :

```text
layouts binaires
pages mappées
I/O direct
FFI GPU
intrinsics CPU
allocateurs spécialisés
structures lock-free
```

Règles :

```text
unsafe encapsulé
unsafe documenté
unsafe testé
unsafe audité
API safe autour
fuzz/property tests
pas de panic serveur critique
MIRI/sanitizers lorsque possible
```

## 13. Profils CPU

| Architecture | Profils |
|---|---|
| x64 | `x64-baseline`, `x64-avx2`, `x64-avx512`, `x64-server-amx`. |
| ARM64 | `arm64-baseline-neon`, `arm64-sve`, `arm64-sve2`, `arm64-server-secure`. |

Détection runtime + policy. Jamais de supposition statique.

### 13.1 Usages CPU

| Usage | Accélération possible |
|---|---|
| WAL checksum | CRC/SHA accéléré si disponible. |
| Encryption | AES-NI/AES ARM ; jamais crypto maison. |
| Scans courts | CPU scalar/SIMD. |
| Bitmap/cardinality | POPCNT/AVX/SVE. |
| Compression | SIMD si gain prouvé. |
| Latches/counters | Atomics, éviter global lock. |

## 14. GPU

Autorisé :

```text
histogrammes
cardinalité
distribution
skew detection
scans analytiques
agrégations massives
benchmark scenarios
vectoriel/embedding si extension contrôlée
```

Interdit :

```text
commit
WAL
rollback
recovery
index lookup ligne par ligne
logique Procedure OLTP
MVCC visibility courte
sécurité critique
```

Le GPU doit être un accélérateur de batch, pas une dépendance du chemin transactionnel.

## 15. NVMe / SSD

Usages :

```text
WAL durable
Hot pages
Version store
Temp store
Stats build
Benchmark temp
Procedure code/cache si nécessaire
```

Métriques :

```text
bytes written
wal bytes
hot page bytes
write amplification
wear percentage
spare available
media errors
unsafe shutdowns
temperature
latency p50/p95/p99
queue depth
```

## 16. ZNS

Zoned Namespaces est intéressant pour :

```text
WAL segmenté
append-only logs
snapshot build
cold construction
compaction contrôlée
```

Position V0 : optionnel, jamais obligatoire. Andromeda doit fonctionner sans ZNS.

## 17. HDD

HDD = ColdStore.

Autorisé :

```text
snapshots
archives
cold indexes
published stats
manifests
forensic copies
backup staging
```

Interdit :

```text
transaction directe
random update critique
WAL actif unique
version store actif
temp intensif
commit path
```

## 18. Roadmap V0

Priorité absolue :

```text
Catalog + SRPL minimal + Transaction Kernel + WAL + Recovery + RPC contractuel
```

Pas le GPU. Pas le benchmark prédictif. Pas l’OLAP sophistiqué.

### 18.1 Phase 0 — Spécifications minimales

```text
Lexique officiel
Catalog object model
Type system v0
Procedure contract v0
RPC frame v0
PageHeader v0
WalRecord v0
Manifest v0
Transaction state machine
```

### 18.2 Phase 1 — Prototype vertical

```text
Create Database
Create Namespace
Create Table
Create Enum
Create StructuredObject
Create Procedure
Call Procedure via RPC local
WAL durable
Commit
Read ResultStream
Procedure Store trace
Crash/recovery test
```

### 18.3 Phase 2 — Storage sérieux

```text
BufferPool
HotStore abstraction
Cold snapshot publication
Manifest switch atomique
WAL replay
MVCC basic
Checkpoint logical
```

### 18.4 Phase 3 — Optimizer minimal

```text
Procedure Plan
Procedure Plan Cache
basic cost model
StatsVersion basic
Procedure Store feedback
```

### 18.5 Phase 4 — Maps et StructuredObject avancés

```text
Map immediate simple
Map incremental
StructuredObject row/column layout
RowCountExact protocol
```

### 18.6 Phase 5 — Administration et Security

```text
mTLS
UserPrincipal
Certificate Registry
Permissions
Admin Surface
Audit Ledger
Debug snapshot
```

### 18.7 Phase 6 — Analytics / Benchmark / GPU

```text
Stats GPU optional
ScenarioEvidence
Predictive Evidence Engine
Procedure Store advanced
Analytical Maps advanced
```

## 19. Principaux risques

| Risque | Gravité | Réponse |
|---|---:|---|
| Trop grand périmètre | Très haute | Prototype vertical strict. |
| Map Immediate trop coûteuse | Haute | Modes refresh + budgets. |
| Float mal utilisé | Haute | Classification approximative + interdits. |
| Procedure Plan explosion | Haute | Quotas, PlanClass, éviction. |
| Catalog central fragile | Très haute | Versioning, WAL, backup, signatures. |
| Benchmark pseudo-vérité | Haute | Validation analytique obligatoire. |
| QUIC custom trop complexe | Moyenne/haute | Framing V0 minimal. |
| Unsafe Rust non maîtrisé | Très haute | Encapsulation + fuzz + audit. |
| Recovery insuffisamment testé | Critique | Crash tests dès Phase 1. |
| Learned components surconfiants | Haute | Fallbacks classiques + evidence non décisionnaire. |
| NVMe wear | Haute | Metrics, budgets, write amplification control. |
| Audit trop volumineux | Moyenne | Retention, compaction signée, tiers froid. |
| HA/DR split-brain | Critique | Quorum/fencing strict, pas d’auto-promotion. |

## 20. Décisions ouvertes

| Sujet | Décision attendue |
|---|---|
| Nom final produit | Andromeda ou nom distinct. |
| Syntaxe SRPL définitive | Forme canonique stricte. |
| Format ContractHash | Canonicalisation, algorithme, version. |
| Endianness binaire | Canonique cross-platform. |
| PageSize par défaut | 16 KiB ou 32 KiB. |
| Decimal internal representation | BCD, scaled integer, custom. |
| Float modes | Rounding, NaN policy, deterministic modes. |
| Map refresh policies | Seuils, budgets, staleness. |
| HA/DR quorum protocol | Majority, witness, weights. |
| Backup encryption/key management | Rotation, escrow, restore. |
| Procedure Code Cache | Interpréteur, JIT, native AOT, invalidation. |
| Statistics publication | Async/sync, validation CPU, rollback stats. |
| Security Maps | Modèle exact de row-level/tenant policy. |
| Contract compatibility | Additive/breaking/deprecation. |

## 21. Corpus scientifique consolidé

Les PDF de fondation rappellent un point essentiel : le noyau scientifique stable n’est pas obsolète. Le relationnel, l’algèbre relationnelle, les transactions, WAL/ARIES, l’optimisation à coût, les statistiques, la normalisation et les B/B+Trees restent des piliers.

### 21.1 Socle relationnel

| Source | Apport pour Andromeda |
|---|---|
| Edgar F. Codd, modèle relationnel | Indépendance logique/physique, relations, tuples, attributs. |
| Relational completeness | Base formelle de l’expressivité. |
| System R / Selinger | Optimisation à coût : cardinalité, coût, énumération de plans. |
| Foundations of Databases | Cadre théorique relationnel/calcul/algèbre. |

Doctrine Andromeda : conserver le relationnel comme fondation, mais ne pas importer SQL ad hoc comme surface native.

### 21.2 Transactions et recovery

| Source | Apport |
|---|---|
| Jim Gray, transaction concept | Transaction comme transformation d’état fiable. |
| Härder/Reuter ACID | ACID comme propriétés reliées aux mécanismes. |
| Eswaran/Gray/Lorie/Traiger | 2PL, predicate locks, phantoms. |
| Berenson et al. critique ANSI isolation | Les labels d’isolation sont insuffisants. |
| Adya | Définitions généralisées des anomalies/isolation. |
| ARIES / Mohan | WAL, UNDO/REDO, recovery robuste. |
| SSI PostgreSQL | Snapshot Isolation ne suffit pas toujours ; SSI donne sérialisabilité. |
| Calvin | Déterminisme transactionnel distribué dans des contextes ciblés. |

Doctrine Andromeda : parler en anomalies et mécanismes, pas en slogan ACID.

### 21.3 Normalisation et dépendances

| Source | Apport |
|---|---|
| Codd further normalization | Réduction anomalies insertion/update/delete. |
| Bernstein 3NF synthesis | Dépendances fonctionnelles et synthèse. |
| Fagin 4NF/DK/NF | Dépendances multivaluées et idéal de contraintes domaines/clés. |
| Cardinality constraints | Différence cardinalité conceptuelle vs statistique. |
| Kimball / summarizability | Grain analytique et agrégation correcte. |

Doctrine Andromeda : normaliser pour OLTP et déclarer le grain pour analytics.

### 21.4 Optimisation/statistiques

| Source | Apport |
|---|---|
| Poosala histogrammes | Estimation de sélectivité par histogrammes. |
| Volcano Optimizer | Recherche extensible de plans. |
| Synthèses cost-based 2021/2025 | Les erreurs de cardinalité restent centrales. |
| Learned CE studies | Gains possibles mais robustesse non universelle. |
| Poisoning learned CE | Historique de requêtes = surface d’attaque. |

Doctrine Andromeda : learned components comme evidence contrôlée, pas comme vérité absolue.

### 21.5 Indexation et recherche

| Source | Apport |
|---|---|
| Bayer/McCreight B-Tree | Index ordonné dynamique adapté au stockage paginé. |
| B+Tree transactionnels / ARIES IM | Recovery et concurrence sur indexes. |
| Extendible Hashing | Hachage dynamique pour égalité. |
| Linear Hashing | Croissance progressive du hachage. |
| Knuth optimal BST | Recherche pondérée théorique. |
| Tarjan DFS | Graphes, cycles, dépendances, O(V+E). |
| Kraska learned indexes | Index comme modèle ; intéressant mais contextuel. |

Doctrine Andromeda : B+Tree par défaut pour ordonné ; learned indexes expérimentaux avec fallback.

### 21.6 Systèmes et hardware

| Source | Apport |
|---|---|
| Abadi/Boncz/Harizopoulos column stores | Compression, columnar analytics, vectorization. |
| SQL Server In-Memory/native compilation | Référence industrielle partielle sur procédures compilées. |
| RFC 9000 QUIC | Transport multiplexé sécurisé. |
| PostgreSQL WAL/MVCC docs | Références pratiques. |
| SQLite WAL | WAL simple et concret. |
| MyRocks LSM | LSM pour write-heavy, compaction. |
| LMDB CoW MVCC | Copy-on-write mmap, MVCC. |
| Rust book/nomicon/unsafe guidelines | Ownership, concurrency, unsafe discipline. |
| NVMe ZNS | Zones append-only utiles pour WAL/snapshots. |
| Intel AMX/SIMD | Accélération CPU pour batch/statistiques. |

## 22. Règle de prudence sur les sources

Une idée externe n’est intégrée à Andromeda que si elle peut être :

```text
définie
bornée
observée
versionnée
testée
rejouée
désactivée
récupérée après crash
```

Les sources sérieuses servent de points d’appui. Elles ne dictent pas Andromeda.

## 23. Prochaine étape recommandée

La prochaine étape utile n’est pas d’ajouter de nouvelles ambitions. Elle est de produire des spécifications normatives courtes et testables :

1. `ProcedureContract v0`.
2. `TypeSystem v0`.
3. `WalRecord v0`.
4. `PageHeader/PageTrailer v0`.
5. `DatabaseManifest v0`.
6. `FrameHeader/RPC v0`.
7. `TransactionStateMachine v0`.
8. `CatalogObjectModel v0`.
9. `CrashRecoveryTestPlan v0`.

Chaque specification doit contenir : structure, invariants, sérialisation, erreurs, tests, compatibilité et critères de rejet.

## 24. Synthèse du document

Andromeda doit devenir performant par accumulation de preuves, pas par accumulation d’heuristiques opaques.

```text
Procedure Store observe.
Statistics consolide.
Predictive Evidence explore.
Optimizer décide.
Hardware accélère.
Policies bornent.
Observability prouve.
Recovery tranche la vérité.
```



## 25. Opérateurs physiques initiaux

| Opérateur | Rôle | Notes |
|---|---|---|
| TableScan | Scan relationnel complet. | Row ou column selon layout. |
| IndexSeek | Recherche par clé/range. | B+Tree/HotIndex. |
| Filter | Application prédicat. | Pushdown prioritaire. |
| Projection | Réduction colonnes. | Évite payload inutile. |
| NestedLoopJoin | Join petit/paramétré. | Bon avec index côté inner. |
| HashJoin | Join gros equality. | Attention mémoire/spill. |
| MergeJoin | Join ordonné. | Bon si inputs ordonnés. |
| AggregateHash | Group by hash. | Risque spill si cardinalité sous-estimée. |
| AggregateStream | Group by stream ordonné. | Faible mémoire. |
| Sort | Ordering contractuel. | Coût mémoire/temp. |
| MapLookup | Lecture Map matérialisée. | Dépend refresh/staleness. |
| DmlUpdate | Mutation avec RowsAffected. | WAL obligatoire. |
| ResultStreamBuild | Metadata + payload. | Backpressure aware. |

## 26. Modèle de coût V0

Un modèle de coût minimal peut commencer par :

```text
TotalCost = CpuCost + LogicalIoCost + PhysicalIoCost + WalCost + TempCost + NetworkCost + RiskPenalty
```

| Terme | Dépendances |
|---|---|
| CpuCost | opérateurs, lignes estimées, SIMD possible. |
| LogicalIoCost | pages logiques, buffer hit attendu. |
| PhysicalIoCost | HotStore/ColdStore, random/sequential, storage temperature. |
| WalCost | bytes WAL, flush policy, mirror/sync replica. |
| TempCost | sort/hash spill, NVMe pressure. |
| NetworkCost | ResultStream size, client speed, batch size. |
| RiskPenalty | cardinality uncertainty, skew, stale stats, plan instability. |

Le coût doit rester explicable. Une formule approximative mais traçable vaut mieux qu’une boîte noire non diagnostiquable.

## 27. Statistics Catalog

```text
StatsObject {
    StatsId,
    ObjectId,
    StatsVersion,
    ColumnSet,
    RowCount,
    DistinctCount,
    NullOrAbsentCount,
    Histogram,
    Density,
    SkewScore,
    CorrelationHints,
    SampleRate,
    BuiltAt,
    ValidatedAt,
    ValidationState,
    SourceSnapshotId
}
```

États : `Candidate`, `Validating`, `Published`, `Rejected`, `Expired`, `Superseded`.

Publication : une StatsVersion candidate ne devient active qu’après validation et switch contrôlé. Les plans doivent enregistrer la StatsVersion consommée.

## 28. Lifecycle de ScenarioEvidence

```text
Proposed
  -> Scheduled
  -> Running
  -> Validating
  -> Accepted
  -> Consumed
  -> Expired
  -> Archived
```

Rejet possible à tout moment si : stats périmées, scenario irréaliste, coût benchmark trop élevé, drift détecté, conflit de policy ou résultat contradictoire avec le réel Procedure Store.

## 29. Classification des jobs GPU

| Classe | Autorisé | Priorité |
|---|---|---:|
| GPU_STATS | Histogrammes, cardinalité, skew. | Basse à moyenne. |
| GPU_ANALYTICS | Scans/aggrégations Maps SnapshotOnly. | Basse hors fenêtre. |
| GPU_BENCHMARK | Scénarios prédictifs. | Basse, annulable. |
| GPU_VECTOR | Similarité/vectoriel si extension. | Basse, isolée. |
| GPU_COMMIT | Interdit. | N/A. |
| GPU_RECOVERY | Interdit. | N/A. |

Un job GPU doit être annulable sans compromettre transaction, recovery ou catalogue.

## 30. Backlog d’ingénierie avec critères d’acceptation

| Élément | Critère d’acceptation |
|---|---|
| Parser SRPL minimal | Parse 20 procédures exemples, diagnostics stables. |
| Binder types/cardinalité | Détecte singleton non garanti, optional non traité. |
| ContractHash v0 | Hash stable sur formes canoniques, tests golden. |
| RPC local harness | Execute Procedure avec StructuredObject + ResultStream. |
| WAL v0 | Crash tests avant/après commit passent. |
| Recovery v0 | Reconstruit DB après kill -9 injecté. |
| CatalogVersion v0 | DefinitionBatch rollback complet sur erreur. |
| Procedure Store v0 | Enregistre Duration, RowsAffected, WalBytes, ErrorKind. |
| Stats v0 | Histogramme simple publié avec StatsVersion. |
| Plan Cache v0 | Invalidation sur CatalogVersion/StatsVersion. |
| Security v0 | mTLS simulé ou réel + UserPrincipal + permission ExecuteProcedure. |
| Backup v0 | Snapshot + WAL replay testés en restore séparé. |

## 31. Références structurantes à conserver dans le dossier source

| Domaine | Références prioritaires |
|---|---|
| Relationnel | Codd 1970, relational completeness, Foundations of Databases. |
| Optimisation | Selinger/System R, Volcano Optimizer, synthèses cost-based modernes. |
| Transactions | Gray, Härder/Reuter, Berenson, Adya, SSI, Calvin. |
| Recovery | ARIES, PostgreSQL WAL, SQLite WAL. |
| Normalisation | Codd normalization, Bernstein 3NF, Fagin 4NF/DK/NF. |
| Indexation | B-Tree/B+Tree, Extendible Hashing, Linear Hashing, learned indexes. |
| Analytics | Column stores, summarizability, histograms, cardinality estimation. |
| Hardware | Rust ownership/unsafe, QUIC RFC 9000, NVMe ZNS, SIMD/AMX. |

La lecture recommandée est d’abord le socle historique, ensuite le front moderne. Ne pas inverser : les composants learned doivent être évalués contre des baselines classiques fortes.


---

## Annexe locale — règle de consolidation

Cette version consolide les fichiers projet et les trois PDF de fondation sans tenter de transformer Andromeda en moteur SQL généraliste. Les équivalences SQL restent pédagogiques. La surface native reste :

```text
QUIC + RPC custom + Procedure cataloguée + SRPL + contrats typés + WAL/MVCC/recovery
```

Toute extension future doit rester définissable, déterministe ou explicitement bornée, typée, observable, récupérable après crash, versionnée, explicable et désactivable.

