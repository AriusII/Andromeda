# 03 — Transaction Kernel, WAL, MVCC, Storage et Recovery

> Consolidation : **Andromeda — SGBDRT Moderne 2026**  
> Date : **2026-05-06**  
> Nature : **Markdown final consolidé**  
> Sources intégrées : 18 fichiers Markdown V0, 1 complément Storage Engine, 3 PDF de fondation.  
> Doctrine : relationnel transactionnel, RPC-only, SRPL, typage fort, déterminisme, observabilité, recovery, Enterprise Grade.


## 1. Périmètre du document

Ce document consolide le noyau transactionnel et le moteur de stockage d’Andromeda. Il couvre le Transaction Kernel, les états transactionnels, WAL/LogStream, MVCC, recovery, BufferPool, HotStore NVMe, ColdStore HDD, pages, extents, manifests, snapshots, I/O scheduling, segments contigus et invariants d’immutabilité froide.

Ce bloc est le cœur **mission-critical** du projet. Sans lui, les procédures, l’optimiseur, les Maps, la sécurité et les surfaces réseau ne sont que des abstractions non prouvées.

## 2. Rôle du Transaction Kernel

Le Transaction Kernel est l’autorité sur l’état transactionnel. L’Execution Engine orchestre ; le Storage Engine persiste et reconstruit ; le Transaction Kernel décide de la visibilité, du commit, du rollback et de la transition d’état.

```text
Execution Engine = orchestre
Transaction Kernel = garantit l'état transactionnel
Storage Engine = persiste et reconstruit
WAL = preuve durable de transition
MVCC = visibilité concurrente
Recovery = reconstruction après panne
```

## 3. États transactionnels

États V0 :

```text
Created
Active
Committing
Committed
Failed
Poisoned
RollingBack
RolledBack
Disposed
```

| État | Sens | Sorties autorisées |
|---|---|---|
| Created | TransactionScope créé, pas encore actif. | Active, Failed. |
| Active | Lectures/mutations autorisées. | Committing, Failed, Poisoned, RollingBack. |
| Committing | Commit en cours, WAL finalisation. | Committed, Failed/Poisoned selon erreur. |
| Committed | Commit visible. | Disposed. |
| Failed | Erreur contrôlée. | RollingBack. |
| Poisoned | État non fiable pour continuer. | RollingBack. |
| RollingBack | Annulation en cours. | RolledBack. |
| RolledBack | Annulation terminée. | Disposed. |
| Disposed | Ressources libérées. | Terminal. |

## 4. Invariant de commit

Invariant C5 :

```text
Commit visible = WAL durable
```

Aucune mutation ne devient visible sans enregistrement durable dans le LogStream. Cela doit rester vrai pour :

| Mutation | Couverture WAL |
|---|---|
| Table row insert/update/delete | Oui. |
| Index insert/delete | Oui. |
| MVCC version create/close | Oui. |
| Map immediate delta | Oui. |
| Catalog change | Oui. |
| Security registry mutation | Oui. |
| Manifest switch | Oui, ou preuve durable équivalente. |

## 5. Vérité système

```text
État canonique = dernier Cold Snapshot valide + WAL durable depuis ce snapshot
```

Interprétation :

| Support | Rôle | Vérité ? |
|---|---|---:|
| RAM | Working set, exécution, dirty pages. | Non. |
| BufferPool | Cache volatile de pages. | Non. |
| NVMe WAL | Vérité récente durable. | Oui pour transitions récentes. |
| NVMe HotStore | Pages chaudes, version store, temp. | Reconstructible. |
| HDD ColdStore | Snapshots publiés, archives, manifests. | Oui pour base froide validée. |
| GPU | Calcul auxiliaire. | Non. |

Le HDD peut être en retard. La RAM peut être perdue. Le NVMe contient la vérité récente. La reconstruction combine snapshot froid et WAL.

## 6. Pipeline d’écriture

```text
Procedure Invocation
  -> TransactionScope
  -> modifications en RAM
  -> WAL records
  -> WAL flush durable
  -> Commit visible
  -> dirty page writeback vers HotStore
  -> checkpoint logique
  -> snapshot froid publié
```

### 6.1 Étapes et responsabilités

| Étape | Autorité | Risque principal |
|---|---|---|
| Invocation | Execution Engine | Contrat invalide. |
| TransactionScope | Transaction Kernel | État incorrect. |
| Modifications RAM | Storage/BufferPool | Perte si non journalisé. |
| WAL append | Log Manager | Record incomplet. |
| WAL flush | I/O Scheduler | Durabilité non acquise. |
| Commit visible | Transaction Kernel | Visibilité prématurée. |
| Dirty writeback | Storage Engine | Write amplification. |
| Checkpoint | Recovery Manager | Troncature trop agressive. |
| Cold snapshot | Snapshot Publisher | Manifest invalide. |

## 7. WAL / LogStream

Le WAL est append-only, segmenté, checksummé, chaîné, rejouable, tronquable et vérifiable.

### 7.1 Propriétés minimales

| Propriété | Exigence |
|---|---|
| Append-only | Pas de réécriture logique du passé. |
| Segmenté | Gestion de rotation, purge, réplication, archiving. |
| Checksummé | Détection de record corrompu. |
| Chaîné | Continuité vérifiable LSN/previous hash. |
| Rejouable | REDO possible depuis snapshot/checkpoint. |
| Tronquable | Seulement après garantie snapshot/replicas/backups. |
| Auditable | Corrélation avec TxId, InvocationId, CatalogVersion. |

### 7.2 Records minimaux

```text
TxBegin
TxCommit
TxRollback
PageAllocate
PageFormat
RowInsert
RowUpdate
RowDelete
IndexInsert
IndexDelete
MvccVersionCreate
MvccVersionClose
MapDeltaRecord
CatalogChangeRecord
SecurityRegistryRecord
CheckpointBegin
CheckpointEnd
SnapshotBegin
SnapshotEnd
ManifestSwitch
```

### 7.3 Structure conceptuelle d’un record

```text
WalRecord {
    Magic,
    FormatVersion,
    Lsn,
    PrevLsn,
    TxId,
    InvocationId,
    RecordType,
    ObjectId,
    PageId optional,
    PayloadLength,
    Payload,
    Crc64,
    ChainHash
}
```

## 8. MVCC

MVCC permet aux lecteurs de voir un snapshot cohérent sans bloquer toutes les écritures. Modèle conceptuel :

```text
RowHeader {
    RowId,
    BeginTs,
    EndTs,
    CreatorTxId,
    DeleterTxId,
    PrevVersionPtr,
    Flags
}

visible(row, snapshotTs) =
    row.BeginTs <= snapshotTs
    && (row.EndTs == INF || row.EndTs > snapshotTs)
```

## 9. Isolation et anomalies

Andromeda doit documenter les garanties en termes d’anomalies, pas seulement par labels. Le rapport scientifique consolidé rappelle que “ACID” ne suffit pas : l’isolation réelle dépend des anomalies interdites, du protocole de concurrence et du modèle de visibilité.

| Niveau | Sens | Anomalies typiques restantes | Position Andromeda |
|---|---|---|---|
| Read Committed | Pas de dirty read. | Non-repeatable read, phantoms, write skew selon moteur. | Possible pour procédures non critiques. |
| Repeatable Read | Stabilité de lignes relues. | Phantoms selon implémentation. | À définir précisément. |
| Snapshot Isolation | Snapshot cohérent + pas de dirty read. | Write skew, pas forcément serializable. | Utile mais à ne pas confondre avec serializable. |
| Serializable | Équivalent à un ordre sériel. | Coût/blocage/abort plus élevés. | Niveau de référence pour procédures critiques. |

SRPL doit permettre de déclarer ou déduire une isolation policy. Une Procedure critique ne doit pas hériter d’un niveau implicite non audité.

## 10. GC MVCC

Le GC ne supprime une version que si :

```text
version.EndTs < OldestActiveSnapshotTs
AND aucune Procedure active ne détient encore de pin/reader
AND la version n’est pas requise par recovery, backup ou forensic window
```

Garde-fous :

| Risque | Réponse |
|---|---|
| Long reader bloque GC | Quota snapshot, alerte, kill policy. |
| Backup en cours | Pin durable de fenêtre WAL/version. |
| Replica en retard | Retention WAL/version adaptée ou resync snapshot. |
| Forensic mode | Conservation renforcée. |

## 11. Recovery

Algorithme conceptuel :

```text
1. Lire manifest actif.
2. Vérifier signature/hash/CRC.
3. Monter dernier snapshot froid valide.
4. Lire WAL depuis RequiredWalStartLsn.
5. Rejouer REDO des records complets et valides.
6. Identifier transactions incomplètes.
7. Annuler transactions incomplètes.
8. Reconstruire hot maps/indexes nécessaires.
9. Vérifier invariants catalogue, storage, transactions.
10. Ouvrir la Database en Online, ReadOnly ou ForensicOnly selon résultat.
```

### 11.1 REDO / UNDO

Andromeda peut s’inspirer de la discipline ARIES : WAL, REDO, UNDO, checkpoints et traitement des transactions incomplètes. L’objectif n’est pas de copier aveuglément une implémentation historique, mais de conserver l’invariant : le recovery doit être déterministe, vérifiable et testé par crash scenarios.

### 11.2 Cas critiques

| Cas | Réponse V0 |
|---|---|
| WAL tronqué | Lire jusqu’au dernier record CRC-valid ; marquer recovery degraded. |
| Manifest corrompu | Tenter previous manifest ; forensic trace obligatoire. |
| NVMe perdu | Revenir au dernier snapshot HDD, sauf miroir WAL disponible. |
| Snapshot froid invalide | Essayer snapshot précédent + WAL correspondant. |
| Transaction incomplète | UNDO/rollback. |
| Catalog incohérent | Ouvrir en ForensicOnly ou refuser ouverture. |
| Mission critical | 2 NVMe miroir minimum pour WAL. |

## 12. Storage Engine : principe hot/cold

```text
RAM active -> NVMe transactionnel/chaud -> HDD froid consolidé
```

Le HDD ne reçoit jamais la pression OLTP directe.

| Niveau | Usage | Interdits |
|---|---|---|
| RAM/BufferPool | Working set, dirty pages, plan/runtime buffers. | Vérité durable. |
| HotStore NVMe | WAL, pages chaudes, version store, temp, proc cache. | Être l’unique vérité longue durée sans snapshot. |
| ColdStore HDD | Snapshots publiés, manifests, archives, cold indexes. | WAL actif unique, random update critique, temp intensif. |

## 13. Lecture/écriture storage

### 13.1 Lecture

Ordre typique :

```text
BufferPool
  -> HotStore NVMe
  -> ColdStore HDD
  -> merge iterator hot+cold si nécessaire
```

### 13.2 Écriture

```text
RAM dirty page
  -> WAL durable
  -> page flushable
  -> HotStore
  -> checkpoint
  -> Cold Snapshot publication
```

La page peut être écrite après le commit, mais la transition logique doit être dans le WAL durable avant visibilité.

## 14. Pages et extents

PageSize recommandé V0 : 16 KiB ou 32 KiB. Extent : 64 pages.

```text
Page = PageHeader + Payload + FreeSpace/SlotDirectory + PageTrailer
```

### 14.1 PageHeader conceptuel

```text
PageHeader {
    Magic,
    FormatVersion,
    PageType,
    FileId,
    PageId,
    ObjectId,
    AllocationUnitId,
    PageLsn,
    PageEpoch,
    PrevPageId,
    NextPageId,
    LowerOffset,
    UpperOffset,
    FreeBytes,
    SlotCount,
    RowCount,
    Flags,
    HeaderCrc32
}
```

### 14.2 PageTrailer

```text
PageTrailer {
    PayloadCrc64,
    PageHash,
    TornWriteGuardA,
    TornWriteGuardB
}
```

## 15. Layouts de pages

| Layout | Usage | Structure |
|---|---|---|
| FixedRowPage | Lignes fixes, densité élevée. | OccupancyBitmap + Row[N]. |
| HybridRowPage | Lignes variables. | FixedPartArea + VariableHeap + OffsetDirectory. |
| LOB/Overflow | Grandes valeurs. | OverflowPointers + segments dédiés. |
| ColumnSegmentPage | Analytics/Maps columnar. | Encodage colonne + dictionary/compression. |

Règle : la partie fixe doit rester dense. Les pointeurs d’overflow doivent être audités et couverts par WAL.

## 16. AccessPaths hot/cold

```text
ColdIndex = B+Tree dense, stable, HDD
HotIndex  = Delta Tree mutable, NVMe
Lecture   = merge iterator hot + cold
```

Le B+Tree reste le choix ordonné par défaut pour les accès transactionnels. Les hash indexes peuvent être utiles pour l’égalité, mais ne remplacent pas les parcours ordonnés ni les ranges. Les learned indexes restent expérimentaux/optionnels et ne doivent pas entrer dans le noyau C5 sans preuve robuste.

## 17. Manifest

Structure conceptuelle :

```text
DatabaseManifest {
    DatabaseId,
    ManifestVersion,
    SnapshotId,
    BaseCheckpointLsn,
    RequiredWalStartLsn,
    CatalogRoot,
    ObjectCount,
    ObjectManifestsHash,
    FileListHash,
    PreviousManifestHash,
    CreatedAt,
    ManifestCrc64,
    Signature
}
```

Le manifest est le point d’entrée de recovery. Il doit être petit, vérifiable, signé et relié au previous manifest.

## 18. Snapshot publication

Règle : jamais d’update-in-place du snapshot froid.

```text
snapshot actif
  -> snapshot tmp
  -> validation
  -> manifest.next
  -> fsync
  -> atomic switch
  -> cleanup différé
```

À tout moment, au moins un snapshot froid valide doit exister.

## 19. Invariants de fichiers, segments et contiguïté

Le complément Storage Engine renforce une règle : le ColdStore doit être append-only, immutable et physiquement lisible de façon séquentielle.

### 19.1 Invariant critique

```text
Aucun split physique de page après écriture froide.
Une page écrite dans un segment froid est immutable.
Une modification implique nouvelle version.
Aucun update-in-place dans ColdStore.
```

### 19.2 Modèle de stockage

```text
Manifest -> SegmentIndex -> SegmentFiles -> Blocks -> Pages
```

### 19.3 Segment contigu

```text
[Header][Block][Block][Block][Trailer]
```

Interdits :

```text
fragmentation interne
réécriture partielle
trous logiques
page fragmentée
compression globale inter-blocs
```

### 19.4 Block

```text
BlockHeader
Payload compressé ou non
BlockTrailer
```

Compression : par bloc uniquement. Jamais globale, pour préserver lecture indépendante, validation et recovery partiel.

## 20. Pipeline d’écriture segment froid

```text
1. Construction en RAM.
2. Append dans segment staging.
3. Flush.
4. CRC/hash validation.
5. Seal segment.
6. Publication via manifest.
```

Cette discipline évite la corruption silencieuse, simplifie le recovery et favorise les lectures séquentielles HDD.

## 21. Maintenance storage

Pipeline de maintenance :

```text
1. Fence.
2. Backup.
3. Reconstruction segments contigus.
4. Validation.
5. Manifest switch atomique.
6. Cleanup différé.
```

La maintenance ne doit pas casser les garanties de disponibilité ou de recovery. Elle doit respecter les fenêtres de WAL/backup/replica.

## 22. I/O Scheduler

Priorités recommandées :

| Priorité | Type I/O |
|---:|---|
| P0 | WAL flush. |
| P1 | Transaction read miss. |
| P2 | Index root/internal read. |
| P3 | Hot page writeback. |
| P4 | Checkpoint. |
| P5 | Cold snapshot build. |
| P6 | Scrub/maintenance. |

WAL flush ne doit jamais être affamé par un refresh de Map, un build statistique ou une tâche GPU.

## 23. Temp, spill et quotas

Le Temp Store doit être considéré comme une ressource critique de stabilité. Il n’est pas vérité, mais il peut dégrader fortement le système.

| Ressource | Risque | Garde-fou |
|---|---|---|
| TempBytes | Saturation NVMe. | Quotas par session/procedure. |
| SpillBytes | Plans mal estimés. | Feedback Procedure Store. |
| Version Store | Long readers. | Snapshot quotas. |
| WAL bytes | Latence commit. | Group commit, mirror, priorité P0. |
| Checkpoint backlog | Recovery trop long. | Checkpoint policy. |

## 24. Tests obligatoires

| Test | Objectif |
|---|---|
| Crash avant WAL flush | Mutation non visible. |
| Crash après WAL flush avant dirty page | Mutation visible après recovery. |
| WAL record tronqué | Recovery s’arrête proprement au dernier record valide. |
| Manifest corrompu | Fallback previous manifest. |
| Snapshot tmp incomplet | Non publié. |
| MVCC long reader | Version GC bloqué correctement. |
| Catalog batch crash | Aucun demi-catalogue publié. |
| Map immediate crash | Table + Map atomiques. |
| NVMe perte simulée | Recovery selon miroir/snapshot. |
| Torn write page | Détection via trailer/hash. |

## 25. Décisions ouvertes

| Sujet | Choix à figer |
|---|---|
| PageSize | 16 KiB vs 32 KiB selon workload. |
| Endianness | Format binaire canonique. |
| Crc/hash | CRC32/CRC64/xxhash/SHA selon zone. |
| WAL mirror | Mode sync, quorum local, latence. |
| Checkpoint policy | LSN, temps, bytes, pressure. |
| MVCC timestamp | Epoch, logical timestamp, hybrid clock. |
| Compression | Algorithmes par bloc, dictionnaires, CPU budget. |
| HotIndex structure | Delta tree, B+Tree mutable, Bw-tree-like, autre. |
| Cold segment size | Taille optimale HDD/NVMe. |

## 26. Synthèse du document

Le noyau transactionnel doit être ennuyeux, strict et testable. Le projet peut être innovant sur SRPL, les contrats, les Maps, le Procedure Store, les statistiques et l’evidence prédictive. Mais le commit, le WAL, le recovery et la vérité système ne doivent jamais devenir expérimentaux.

```text
Pas de WAL durable -> pas de commit visible.
Pas de snapshot valide -> pas de vérité froide.
Pas de recovery testé -> pas de fonctionnalité mission-critical.
Pas de trace -> pas de décision critique acceptable.
```



## 27. Modes de durabilité WAL

| Mode | Description | Usage |
|---|---|---|
| LocalDurable | Flush local NVMe durable avant commit visible. | V0 minimal. |
| MirroredLocalDurable | Deux NVMe locaux ou chemins distincts. | Mission critical single node. |
| SyncReplicaDurable | Replica préférée confirme WAL durable. | RPO très faible. |
| AsyncReplicaDurable | Primary commit après local durable, replica suit. | Latence plus faible, RPO > 0. |
| ArchiveDurable | WAL archivé hors nœud pour backup/PITR. | Protection long terme. |

Le contrat de durabilité doit être une policy. Une Procedure critique peut exiger un mode supérieur, mais le moteur doit refuser l’appel si la policy ne peut pas être satisfaite.

## 28. Checkpoint et rétention WAL

Un checkpoint ne doit jamais être confondu avec un commit. Il réduit le coût de recovery, mais ne remplace pas le WAL.

```text
CheckpointBegin
  -> flush dirty pages éligibles
  -> persist checkpoint metadata
  -> CheckpointEnd
  -> update recovery window
  -> WAL truncation only if safe
```

Conditions de troncature WAL :

| Condition | Pourquoi |
|---|---|
| Snapshot froid publié et vérifié. | Base de reconstruction disponible. |
| Replicas ont appliqué ou sont resync prévues. | Ne pas casser HA/DR. |
| Backups/PITR retention respectés. | Restaurabilité. |
| Forensic retention respectée. | Enquête post-incident. |
| Aucun reader/version pin dépendant. | MVCC cohérent. |

## 29. Rapport de validation recovery

Après recovery, Andromeda doit produire un rapport structuré :

```text
RecoveryReport {
    DatabaseId,
    StartMode,
    SnapshotId,
    ManifestVersion,
    RequiredWalStartLsn,
    LastValidWalLsn,
    RedoRecordsApplied,
    TransactionsCommitted,
    TransactionsRolledBack,
    CorruptRecordsSkipped,
    IndexesRebuilt,
    MapsValidated,
    CatalogVersionRecovered,
    SecurityAuditStatus,
    OpenMode,
    Warnings[],
    Errors[]
}
```

Ce rapport sert au démarrage, au support, au forensic et aux tests automatisés.

## 30. BufferPool : exigences V0

| Élément | Exigence |
|---|---|
| Page pinning | Empêcher éviction pendant lecture/mutation. |
| Dirty tracking | Suivre pages modifiées et PageLsn. |
| Eviction policy | LRU/clock/adaptive, mais observable. |
| Checkpoint cooperation | Sélectionner pages flushables. |
| Read-ahead | Activable pour scans. |
| Write coalescing | Réduire I/O random. |
| NUMA awareness | Optionnel mais utile x64 serveur. |
| Metrics | hit ratio, dirty ratio, eviction, stalls. |

Le BufferPool ne doit jamais mentir sur la durabilité : une page dirty en RAM ne rend pas une mutation durable.

## 31. Opérations d’index et WAL

Une mutation de ligne peut entraîner plusieurs records : row, index, MVCC, Map. Le moteur doit choisir une granularité WAL qui rend recovery robuste sans exploser les bytes.

```text
RowUpdate
  -> MvccVersionClose ancienne version
  -> MvccVersionCreate nouvelle version
  -> IndexDelete anciennes clés si clé changée
  -> IndexInsert nouvelles clés si clé changée
  -> MapDeltaRecord si Map immédiate/incremental
```

Pour les indexes froids, les changements peuvent être accumulés dans un HotIndex delta puis fusionnés lors de snapshot/maintenance.

## 32. Scrub et validation ColdStore

Un scrub périodique doit vérifier :

```text
manifest chain
segment headers/trailers
block CRC/hash
page trailer
PageLsn monotonic expectations
object manifest hashes
index/table consistency samples
Map/table consistency selon policy
```

Le scrub doit être P6, jamais prioritaire sur WAL/transactions, mais il est indispensable pour détecter corruption silencieuse HDD.

## 33. Modèle de corruption et réponses

| Corruption | Détection | Réponse |
|---|---|---|
| WAL record partiel | CRC/length/chain. | Stop replay au dernier valide. |
| WAL record au milieu corrompu | CRC/chain mismatch. | ForensicStart ou restore selon policy. |
| Page torn write | Trailer guards/hash. | Recharger depuis WAL/snapshot. |
| Segment froid altéré | Block hash/manifest hash. | Snapshot précédent ou backup. |
| Catalog object incohérent | Catalog invariant validation. | ForensicOnly. |
| Index divergence | Consistency check. | Rebuild index si table saine. |
| Map divergence | Source table compare/delta replay. | Rebuild/refresh Map. |

## 34. Crash-test matrix recommandée

| Moment du crash | Résultat attendu |
|---|---|
| Avant TxBegin durable | Transaction inexistante. |
| Après TxBegin, avant mutation WAL | Rollback/ignore transaction incomplète. |
| Après RowUpdate WAL, avant TxCommit | Undo/rollback. |
| Après TxCommit durable, avant ack client | Transaction committed après recovery ; client peut recevoir état incertain. |
| Après ack client, avant dirty page flush | Transaction visible après recovery via WAL. |
| Pendant manifest switch | Ancien ou nouveau manifest valide, jamais demi-manifest. |
| Pendant Map delta apply | Cohérence selon WAL records et refresh policy. |


---

## Annexe locale — règle de consolidation

Cette version consolide les fichiers projet et les trois PDF de fondation sans tenter de transformer Andromeda en moteur SQL généraliste. Les équivalences SQL restent pédagogiques. La surface native reste :

```text
QUIC + RPC custom + Procedure cataloguée + SRPL + contrats typés + WAL/MVCC/recovery
```

Toute extension future doit rester définissable, déterministe ou explicitement bornée, typée, observable, récupérable après crash, versionnée, explicable et désactivable.

