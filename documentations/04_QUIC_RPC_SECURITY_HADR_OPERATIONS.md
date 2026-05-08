# 04 — QUIC/RPC, sécurité, IAM, administration, HA/DR et opérations

> Consolidation : **Andromeda — SGBDRT Moderne 2026**  
> Date : **2026-05-06**  
> Nature : **Markdown final consolidé**  
> Sources intégrées : 18 fichiers Markdown V0, 1 complément Storage Engine, 3 PDF de fondation.  
> Doctrine : relationnel transactionnel, RPC-only, SRPL, typage fort, déterminisme, observabilité, recovery, Enterprise Grade.


## 1. Périmètre du document

Ce document consolide la surface réseau, le protocole RPC custom, les contrats exposés, la sécurité, l’IAM, l’audit, l’administration, HA/DR, backup/restore et forensic. Il décrit comment le monde externe est autorisé à toucher Andromeda.

La règle de base est simple : Andromeda n’expose pas une surface SQL libre. Il expose des surfaces QUIC séparées, des frames typées, des contrats de Procedure, des StructuredObjects et des ResultStreams.

### 1.1 Lot 5 Acceptance Alignment

Lot 5 acceptance follows DEC-040 and DEC-041. The accepted claim is boundary governance and contract vocabulary, not a complete production network, IAM, or audit runtime.

- `andromeda-rpc-protocol` is the runtime-free RPC frame and stream contract layer. It must not depend on Quinn, Rustls, Tokio, listener lifecycles, storage, WAL, or recovery.
- `andromeda-quic` is the concrete QUIC runtime boundary. Quinn, Rustls, and Tokio belong behind feature-gated transport adapters and must not define Procedure semantics, storage truth, or authorization policy.
- `andromeda-security-contract` is security contract vocabulary: surfaces, permission families, operation classes, stable labels, and semantic mappings. It is not the IAM runtime, policy store, revocation store, durable `PrincipalRegistry`, or certificate extraction runtime.
- Protobuf message contracts and custom frames are allowed. gRPC, tonic, generated gRPC services, ad hoc SQL application surfaces, generic command tunnels, and runtime JSON defaults are not allowed.
- `SecurityAdmission v0` is the pre-transaction admission contract. Protocol validation, surface validation, Procedure contract binding, principal/policy evidence, resource budget checks, and audit evidence must be established before transaction creation. This does not claim full durable IAM implementation.
- `AuditLedger v0` is append-only and checksum chained at the durable audit record layer. Retention compaction may rewrite a compacted journal by retaining records and rethreading chain evidence; compaction is not the transaction commit path and audit evidence is not database truth.
- Do not describe an implemented Admin RPC audit query endpoint unless code proves it. Current operator wording must use `andromeda-cli audit inspect`, `andromeda-cli audit verify`, and `andromeda-cli audit compact`.

Lot 5 acceptance documentation must preserve these limits even when future implementation lots add runtime behavior.

Normative WR-5.DOC links:

- `docs/adr/ADR-0012-quic-rpc-no-grpc.md`
- `documentations/specs/FrameHeader_RPC_v0.md`
- `documentations/specs/SecurityAdmission_v0.md`
- `documentations/specs/AuditLedger_v0.md`

## 2. Décision réseau

Andromeda utilise QUIC comme transport et un protocole applicatif custom fortement typé.

```text
QUIC = transport sécurisé, multiplexé, flow-controlled.
Andromeda RPC = contrats, framing, metadata, payloads typés.
```

QUIC apporte des primitives utiles : connexions sécurisées, multiplexage, streams, flow control, réduction de certains blocages de transport. Mais QUIC ne définit pas la sémantique d’Andromeda. Cette sémantique est dans le protocole RPC maison et le Catalog & Contract Engine.

## 3. Surfaces séparées

Andromeda possède trois surfaces QUIC séparées :

| Surface | Public | Autorisé | Interdit |
|---|---|---|---|
| Application Surface | Applications métier. | HELLO, AUTH, GET CONTRACTS, EXECUTE PROCEDURE, STREAM RESULT, ERROR, telemetry minimale. | Création objets, admin, cluster, maintenance, debug profond. |
| Administration Surface | DBA, outils admin, jobs. | Import DefinitionBatch, plans, Procedure Store, backup/restore, debug snapshot, certificats, policies. | Accès non audité, bypass sécurité. |
| HA/DR Cluster Surface | Nœuds cluster, witnesses, agents réplication. | WAL shipping, health, quorum, fencing, manifests, promotion. | Auto-promotion sans quorum, opérations applicatives métier. |

Chaque surface possède :

```text
certificats autorisés
endpoints
permissions
quotas
priorités
audit
contrats visibles
policies de backpressure
```

## 4. Application Surface

La surface applicative est volontairement restrictive.

### 4.1 Opérations autorisées

```text
HELLO
AUTH
GET_CONTRACTS
EXECUTE_PROCEDURE
STREAM_RESULT
ERROR
TELEMETRY_MINIMAL
PING/HEALTH minimal si autorisé
```

### 4.2 Opérations interdites

```text
CreateTable
CreateMap
CreateProcedure
ImportDefinitionBatch
DebugProcedure
ReadProcedureStore profond
ManageSecurity
Backup
Restore
ClusterPromote
Inspect raw pages/WAL
```

L’application consomme des procédures. Elle ne pilote pas le moteur.

## 5. FrameHeader

Structure conceptuelle :

```text
FrameHeader {
    FrameType,
    RequestId,
    SessionId,
    TxId,
    PayloadLength,
    Flags,
    HeaderCrc
}
```

Types V0 :

| FrameType | Rôle |
|---|---|
| HELLO | Négociation initiale. |
| AUTH | Authentification session. |
| CONTRACT_REQUEST | Demande de contrat. |
| CONTRACT_RESPONSE | Réponse contrat/version. |
| RPC_EXECUTE_REQUEST | Appel Procedure. |
| RPC_METADATA | Metadata avant payload. |
| RPC_BATCH | Batch de payload. |
| RPC_COMPLETION | Fin de flux. |
| ERROR | Erreur typée. |
| TELEMETRY | Signal minimal client/serveur. |

## 6. Metadata avant payload

Pour un `ResultStream`, les metadata précèdent toujours le payload.

```text
Result metadata
RowCountExact/TotalRowsExact si contractuel
Column descriptors
Batch descriptors
Payload batches
Completion
```

Cette règle permet au client de vérifier shape, contrat, cardinalité, layout et stratégie mémoire avant de consommer de gros volumes.

## 7. StructuredObject protocolaire

```text
StructuredObjectHeader {
    Name,
    ContractHash,
    RowCountExact,
    ColumnCount,
    Layout,
    PayloadLength
}
```

Layouts :

| Layout | Usage |
|---|---|
| RowMajor | Validation OLTP, petits paramètres. |
| ColumnMajor | Analytics, grosses colonnes, GPU batch hors commit. |
| Hybrid | Mixte. |

## 8. Contrats exposés

Un contrat exposé doit inclure :

```text
ProcedureName
ContractHash
InputShape
StructuredObject contracts
OutputShape
ResultStream metadata
RequiredPermissions
ProtocolLayout
CompatibilityPolicy
CatalogVersion
PolicyVersion
```

Le client ne doit pas découvrir la forme d’un résultat par hasard au runtime. Le contrat doit être consultable et vérifiable avant appel.

## 9. Fast path / slow path

| Chemin | Conditions | Traitement |
|---|---|---|
| Fast path | Petits paramètres, petite réponse, plan chaud, buffers préalloués. | Peu d’allocations, faible overhead, pas de spool. |
| Slow path | Gros payload, streaming, tri, spill, client lent. | Backpressure, segmentation, spool NVMe contrôlé. |

The fast path must never bypass `SecurityAdmission v0`, Procedure contracts, WAL durability, or audit evidence. It only optimizes execution after authorization.

## 10. Backpressure

Déclencheurs :

```text
buffers saturés
client lent
execution saturée
WAL en retard
NVMe sous pression
temp store proche quota
replica lag critique
admin maintenance en cours
```

Actions possibles :

| Action | Effet |
|---|---|
| Ralentir lecture | Réduit pression client/serveur. |
| Réduire batch size | Diminue mémoire et latence des buffers. |
| Spool contrôlé | Déplace temporairement vers NVMe sous quota. |
| Refuser nouveaux RPC | Préserve noyau critique. |
| Fermer session abusive | Protection de disponibilité. |
| Dégrader analytics | Protège OLTP. |

WAL flush et recovery restent prioritaires sur les tâches opportunistes.

## 11. Doctrine sécurité

Chaîne sécurité :

```text
mTLS -> CertificateIdentity -> UserPrincipal -> Roles/Groups -> Permissions -> Policies -> Audit
```

Un certificat prouve une identité cryptographique. Il ne doit pas être l’utilisateur logique. Cette séparation permet : rotation, révocation, expiration, multi-device, séparation humain/service et audit long terme.

Lot 5 separates security vocabulary from IAM runtime behavior:

- Security contract vocabulary belongs in `andromeda-security-contract` and must remain runtime-free.
- IAM runtime behavior belongs to explicit authorization components and future durable registries.
- Documentation must not use `andromeda-security-contract` as shorthand for a mutable IAM store, policy-management runtime, revocation workflow, or certificate lifecycle implementation.

This distinction keeps DEC-041 contract extraction from becoming an implicit security runtime claim.


## 12. CertificateIdentity

Structure conceptuelle :

```text
CertificateIdentity {
    CertificateId,
    Thumbprint,
    Subject,
    Issuer,
    SerialNumber,
    UserId,
    ValidFrom,
    ValidTo,
    RevokedAt,
    Purpose,
    SurfaceScope,
    LastUsedAt
}
```

`SurfaceScope` possible :

```text
Application
Administration
Cluster
BackupAgent
MonitoringAgent
```

Un certificat Application ne peut pas devenir certificat Administration par simple paramètre client.

## 13. UserPrincipal

```text
UserPrincipal {
    UserId,
    LoginName,
    DisplayName,
    Status,
    CreatedAt,
    DisabledAt,
    Groups[],
    Roles[],
    DirectPermissions[],
    Policies[]
}
```

Le `UserPrincipal` est l’identité logique auditée. Les certificats peuvent changer ; l’utilisateur logique reste la référence métier et d’audit.

## 14. Permissions

Actions minimales :

| Permission | Surface principale |
|---|---|
| ExecuteProcedure | Application. |
| ReadContract | Application/Admin. |
| CreateTable | Administration. |
| CreateMap | Administration. |
| CreateProcedure | Administration. |
| ImportDefinitionBatch | Administration. |
| DebugProcedure | Administration. |
| ReadProcedureStore | Administration/Monitoring. |
| ManageSecurity | Administration sécurité. |
| Backup | BackupAgent/Admin. |
| Restore | Admin/DR. |
| ClusterPromote | HA/DR. |

Les permissions doivent être évaluées avec les policies : surface, environnement, fenêtre temporelle, criticité, ressource, database, namespace, object.

## 15. Policies

Exemples de policies :

| Policy | Exemple |
|---|---|
| SurfacePolicy | Le principal X peut utiliser Application mais pas Administration. |
| ProcedurePolicy | Peut exécuter `Sales.CreateOrder` mais pas `Security.DisableUser`. |
| ResourcePolicy | Max TempBytes, max Duration, max RowsReturned. |
| TimePolicy | Actions admin seulement en fenêtre validée. |
| BreakGlassPolicy | Accès urgence, durée courte, audit renforcé. |
| DataPolicy | Row/tenant constraints via Security Maps. |

## 16. Audit

Traces obligatoires :

```text
SecurityAuditTrace
CatalogChangeTrace
AdminOperationTrace
ProcedureInvocationTrace
TransactionTrace
PlanDecisionTrace
RecoveryTrace
ClusterEventTrace
DefinitionBatchApplyTrace
```

Chaque trace doit contenir au minimum :

```text
TraceId
Timestamp moteur
PrincipalId
CertificateId si applicable
Surface
Operation
Target
CatalogVersion/PolicyVersion si applicable
Result
ErrorKind si applicable
CorrelationId/InvocationId
```

For Lot 5, `AuditLedger v0` means DEC-033 durable audit journal evidence. The record layer is append-only and checksum chained. Retention compaction is allowed only as a policy-governed rewrite of retained records with rethreaded chain evidence and preserved retained payload checksum evidence.

Audit records are forensic and authorization evidence. They are not database truth, do not replace the latest valid cold snapshot plus durable WAL, and must not be placed in the transaction commit path.

Operator examples must use `audit inspect`, `audit verify`, and `audit compact`; `audit query` is obsolete wording.


## 17. SuperAdmin et break-glass

Le SuperAdmin est créé au bootstrap. Règles :

```text
non supprimable sans équivalent
permissions critiques audit obligatoires
break-glass possible mais borné et tracé
aucune désactivation globale de l’audit
rotation de credentials/certificats obligatoire
```

Un système Enterprise Grade doit prévoir l’urgence sans créer une porte arrière permanente.

## 18. Administration Surface

La surface d’administration est puissante, donc dangereuse. Elle doit être strictement auditée.

Opérations :

```text
création/modification objets
import DefinitionBatch
compile/debug SRPL
inspect plans
read Procedure Store
jobs maintenance
backup/restore
certificats/IAM
forensic startup
cluster operations selon permissions
```

Admin audit read is an accepted capability target, but this document must not claim an implemented Admin RPC audit query until code evidence proves that endpoint. Current operator-facing examples are CLI audit tooling: `andromeda-cli audit inspect`, `andromeda-cli audit verify`, and `andromeda-cli audit compact`.

The Application Surface must not route these administrative or forensic operations.


## 19. Debug SRPL

Le debug ne doit jamais muter la production active.

Pipeline :

```text
Admin Debug Request
  -> Permission Check
  -> Isolated Snapshot
  -> Debug Context
  -> Step Execution
  -> Debug Trace
  -> Discard Context
```

Les mutations de debug se font dans un contexte jetable. Les traces sont conservées.

## 20. HA/DR : position V0

Modèle V0 :

```text
Single Primary + Replicas
Pas de multi-primary en V0
```

Rôles :

```text
Primary
ReplicaPreferred1
ReplicaPreferred2
ReplicaN
Witness optionnel
```

Le multi-primary est rejeté en V0 car il complexifie fortement conflit, commit, quorum, écriture concurrente et recovery. La priorité est un système fiable, pas une illusion de disponibilité totale.

## 21. Node metadata

```text
Node {
    NodeId,
    Role,
    Priority,
    PromotionEligibility,
    LastSeen,
    LastDurableLsn,
    LastAppliedLsn,
    ReplicationLag,
    HealthState,
    QuorumWeight
}
```

Ces métadonnées doivent être elles-mêmes auditables et protégées contre une auto-promotion abusive.

## 22. Quorum et fencing

Le quorum vérifie :

```text
majorité ou règle configurée
fencing du primary suspect
LSN le plus avancé
éligibilité promotion
priorité
absence de divergence interdite
capacité à accepter le rôle
```

Le fencing est central. Un ancien primary suspect ne doit pas continuer à accepter des écritures concurrentes après promotion d’une replica.

## 23. Failover

Pipeline :

```text
Primary suspect
  -> quorum verification
  -> replica health check
  -> fencing
  -> recovery to consistent LSN
  -> promotion
  -> cluster manifest update
  -> replicas repoint
```

Un failover non prouvé par traces doit être considéré comme dangereux.

## 24. Replication

V0 : WAL shipping prioritaire.

| Mode | Usage |
|---|---|
| WAL shipping sync | Replica favori, RPO faible, latence commit plus élevée. |
| WAL shipping async | Replica distante, latence moindre, RPO > 0. |
| Snapshot shipping | Bootstrap/resync. |
| Catalog/manifest shipping | Cohérence contractuelle cluster. |

RPO/RTO doivent être explicites. Une configuration sans RPO/RTO déclaré est incomplète.

## 25. Backup ≠ HA/DR

HA/DR protège la disponibilité. Backup protège contre :

```text
erreur humaine
corruption logique
suppression accidentelle
ransomware
bug applicatif
mauvaise migration
compromission silencieuse
```

Une replica qui applique une suppression erronée n’est pas un backup.

## 26. Backup

Types à sauvegarder :

| Élément | Remarque |
|---|---|
| Cold snapshot | Base de restauration. |
| WAL archives | PITR et replay. |
| Catalog | Contrats, objets, versions. |
| Certificate metadata | Nécessaire audit/restauration, avec prudence clés. |
| Procedure Store | Selon retention/policy. |
| Statistics | Selon policy ; reconstructibles mais utiles. |
| Manifests | Point d’entrée restore. |
| Audit ledger | Critique forensic. |

## 27. Restore / PITR

Pipeline PITR :

```text
1. Choisir snapshot froid.
2. Vérifier manifest/signature/hash.
3. Choisir LSN cible ou timestamp logique.
4. Rejouer WAL jusqu’à cible.
5. Annuler transactions incomplètes.
6. Valider catalog/security/storage invariants.
7. Ouvrir en test, ReadOnly ou production.
8. Émettre RestoreTrace.
```

Un restore doit être régulièrement testé. Un backup non testé est une hypothèse, pas une garantie.

## 28. Forensic startup

Modes :

| Mode | Sens |
|---|---|
| FastStart | Démarrage rapide normal. |
| SafeStart | Démarrage prudent avec vérifications renforcées. |
| ForensicStart | Connexions applicatives bloquées, rapport de cohérence produit. |

ForensicStart doit bloquer les connexions applicatives et produire un rapport de cohérence : catalog, WAL, snapshot, indexes, Maps, security audit, cluster state.

## 29. Runbooks opérationnels minimaux

### 29.1 Incident WAL pressure

```text
Détecter latence WAL flush.
Réduire/refuser nouveaux RPC non critiques.
Suspendre analytics/GPU/build stats.
Augmenter group commit si policy le permet.
Alerter admin.
Tracer incident.
```

### 29.2 Incident client lent

```text
Activer backpressure stream.
Réduire batch size.
Spool contrôlé si quota disponible.
Fermer session abusive après seuil.
Tracer RequestId/SessionId.
```

### 29.3 Incident replica lag

```text
Mesurer LastDurableLsn vs LastAppliedLsn.
Adapter retention WAL.
Dégrader async distant si nécessaire.
Déclencher snapshot resync si retard trop grand.
Empêcher promotion si divergence/lag incompatible.
```

### 29.4 Incident suspicion corruption

```text
Passer database en Suspended ou ForensicOnly.
Bloquer écritures.
Valider manifest/snapshot/WAL.
Comparer indexes/maps avec tables source.
Produire RecoveryTrace/ForensicReport.
Restaurer si nécessaire.
```

## 30. Synthèse du document

La surface externe d’Andromeda doit être aussi stricte que son noyau transactionnel. QUIC ne suffit pas ; la sécurité ne suffit pas ; le catalogue ne suffit pas. C’est leur combinaison contractuelle qui donne la sûreté :

```text
Surface séparée
+ mTLS
+ identité logique
+ permissions
+ contrat de Procedure
+ framing typé
+ audit
+ backpressure
+ recovery/HA explicites
```



## 31. Machine d’état session RPC

```text
Disconnected
  -> Connected
  -> HelloReceived
  -> Authenticated
  -> ContractNegotiated
  -> Ready
  -> Executing
  -> Streaming
  -> Completing
  -> Closed
```

Transitions interdites :

| Transition | Raison |
|---|---|
| Connected -> Executing | Auth/contract manquants. |
| Authenticated -> AdminOperation sur Application Surface | Surface scope incompatible. |
| Streaming -> Executing nouveau RPC sans quota | Risque mémoire/backpressure. |
| Closed -> Streaming | Session terminale. |

## 32. Taxonomie d’erreurs RPC

| Famille | Exemple | Couche |
|---|---|---|
| TransportError | Stream reset, timeout transport. | QUIC. |
| ProtocolError | Frame type invalide, HeaderCrc invalide. | RPC framing. |
| AuthError | Certificat expiré/révoqué. | Security. |
| PermissionError | ExecuteProcedure refusé. | IAM. |
| ContractError | ContractHash incompatible. | Catalog/Contract. |
| PayloadError | StructuredObject mal formé. | RPC payload. |
| ExecutionError | BusinessError SRPL. | Execution. |
| TransactionError | Deadlock/serialization failure. | Transaction Kernel. |
| ResourceError | Backpressure/quota/timeout. | Governance. |
| SystemError | Storage/recovery/corruption. | Core/Storage. |

Une erreur réseau ne doit pas automatiquement révéler des détails internes. Les détails profonds sont réservés à l’Admin Surface avec permissions.

## 33. SecurityAdmission v0 Decision Order

`SecurityAdmission v0` is the pre-transaction contract boundary for Application and Administration admission. It composes protocol, surface, contract, principal, policy, resource, and audit evidence before transaction creation. It is not a claim that durable IAM policy storage is complete.

```text
1. Validate frame and protocol version; reject gRPC, runtime JSON defaults, and generic payload tunnels.
2. Validate mTLS certificate and listener surface.
3. Resolve CertificateIdentity evidence.
4. Check certificate validity, revocation, and SurfaceScope.
5. Resolve UserPrincipal or current principal evidence.
6. Check principal status.
7. Load available roles, groups, direct permissions, and policy evidence.
8. Evaluate explicit deny rules.
9. Evaluate required allow rules for the operation and surface.
10. Check context constraints: surface, database, namespace, time, resource budget.
11. Require SecurityAuditTrace or audit evidence for the accepted or rejected decision.
12. Authorize or fail closed before transaction creation.
```

Explicit deny generally wins over allow, except under formally audited break-glass policy.

## 34. Threat model minimal

| Menace | Défense |
|---|---|
| Certificat volé | Révocation, scopes, expiration courte, audit LastUsed. |
| Replay RPC | SessionId, RequestId, nonce/QUIC protection, idempotency policy. |
| Payload oversized | PayloadLength, quotas, streaming, backpressure. |
| Contract probing | ReadContract permission, rate limit. |
| Admin abuse | Least privilege, audit immuable, break-glass borné. |
| Split-brain cluster | Quorum, fencing, promotion eligibility. |
| Ransomware logique | Backups immuables, PITR, audit. |
| Slow client DoS | Backpressure, batch shrink, session close. |
| Replica poisoning | mTLS cluster, manifest signatures, LSN validation. |

## 35. Policies de backup

| Policy | Description |
|---|---|
| SnapshotInterval | Cadence de snapshots froids. |
| WalArchiveRetention | Durée/volume WAL conservé. |
| ImmutabilityWindow | Fenêtre anti-suppression/ransomware. |
| EncryptionPolicy | Chiffrement backup et gestion clés. |
| RestoreTestCadence | Fréquence de tests restore. |
| ForensicRetention | Conservation audit/WAL/traces critiques. |
| GeoReplication | Copie hors site. |
| AirGapExport | Export hors ligne si exigé. |

Backup et restore doivent être traités comme des procédures administratives contractuelles, pas comme scripts externes non audités.

## 36. ClusterManifest conceptuel

```text
ClusterManifest {
    ClusterId,
    Epoch,
    PrimaryNodeId,
    Members[],
    QuorumPolicy,
    LastCommittedClusterLsn,
    FencingTokensHash,
    PromotionHistoryHash,
    CreatedAt,
    PreviousManifestHash,
    Signature
}
```

L’`Epoch` doit changer lors d’une promotion. Les nœuds doivent refuser d’accepter un primary avec epoch obsolète.

## 37. Métriques opérationnelles essentielles

| Domaine | Métriques |
|---|---|
| RPC | active sessions, requests/s, error rate, p95 latency, backpressure count. |
| Security | auth failures, revoked cert use, denied permissions, break-glass events. |
| WAL | flush latency, bytes/s, queue depth, mirror lag. |
| Transaction | commits/s, rollbacks/s, serialization failures, deadlocks. |
| Storage | buffer hit ratio, dirty pages, NVMe temp bytes, HDD scrub errors. |
| HA/DR | replication lag, last durable LSN, quorum state, failover count. |
| Backup | last successful snapshot, WAL archive age, restore test status. |
| Audit | audit queue lag, retention pressure, immutable export status. |


---

## Annexe locale — règle de consolidation

Cette version consolide les fichiers projet et les trois PDF de fondation sans tenter de transformer Andromeda en moteur SQL généraliste. Les équivalences SQL restent pédagogiques. La surface native reste :

```text
QUIC + RPC custom + Procedure cataloguée + SRPL + contrats typés + WAL/MVCC/recovery
```

Toute extension future doit rester définissable, déterministe ou explicitement bornée, typée, observable, récupérable après crash, versionnée, explicable et désactivable.
