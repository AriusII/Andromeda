# 02 — Type System, SRPL, Procedures et Maps

> Consolidation : **Andromeda — SGBDRT Moderne 2026**  
> Date : **2026-05-06**  
> Nature : **Markdown final consolidé**  
> Sources intégrées : 18 fichiers Markdown V0, 1 complément Storage Engine, 3 PDF de fondation.  
> Doctrine : relationnel transactionnel, RPC-only, SRPL, typage fort, déterminisme, observabilité, recovery, Enterprise Grade.


## 1. Périmètre du document

Ce document décrit la partie la plus visible pour les concepteurs et développeurs : le système de types, les Tables, les Enums, les StructuredObjects, le langage SRPL, les Procedures, les contrats et les Maps. Il consolide aussi les principes issus du PDF de fondation SRPL : langage relationnel procédural strict, syntaxe lisible, sémantique d’ensemble, absence explicite, cardinalité déclarée, portée stricte, compilation vers IR et plan cache.

Andromeda ne doit pas proposer une surface applicative permissive. Il doit proposer une surface **contractuelle**, où l’auteur écrit des intentions relationnelles et transactionnelles que le moteur peut typage-checker, optimiser, tracer, rejouer ou refuser.

## 2. Objectif du Type System

Le Type System réduit l’ambiguïté avant exécution. Une donnée critique doit être :

```text
typée
bornée
sérialisable
vérifiable
versionnable
compatible avec les contrats RPC
compatible avec les formats de stockage
observable dans les traces
```

Le Type System n’est pas une décoration syntaxique. Il est une frontière de sécurité, de compilation, de stockage et d’optimisation.

## 3. Catégories de types

| Catégorie | Rôle | Exemples |
|---|---|---|
| Scalars primitifs | Valeurs atomiques bas niveau. | `i32`, `i64`, `u64`, `bool`, `timestamp`, `uuid`. |
| Decimal exact | Valeurs exactes avec précision/scale. | argent, comptabilité, quantités exactes. |
| Float contrôlé | Approximation assumée. | mesure, score, statistiques, analytics. |
| Text | Texte avec encoding et policy. | `utf8`, `utf16`, `unicode`. |
| Binary | Payload binaire borné. | hash, blob borné, signature. |
| Domain type | Type métier dérivé. | `CustomerId`, `OrderNumber`, `EmailAddress`. |
| Enum | Valeurs discrètes cataloguées. | `Role`, `OrderStatus`. |
| Enum flags | Masque validé par type. | `Permission flags`. |
| StructuredObject | Shape tabulaire typé. | `OrderLineInput`. |
| Relation/Table row | Ligne relationnelle persistante. | `Security.User`. |
| Optional | Absence explicite. | `optional Customer`. |
| Collection | Cardinalité typée. | `many OrderLine`, `nonempty many Item`. |

## 4. Types numériques

Les largeurs doivent être explicites. Les conversions implicites dangereuses sont interdites.

| Famille | Types V0 | Règles |
|---|---|---|
| Entiers signés | `i8`, `i16`, `i32`, `i64`, option `i128` | Overflow policy déclarée. |
| Entiers non signés | `u8`, `u16`, `u32`, `u64`, option `u128` | Pas de conversion silencieuse signed/unsigned. |
| Decimal | `decimal.min`, `decimal.mid`, `decimal.max`, `decimal.custom(p,s)` | Exact ; requis pour argent. |
| Float | `float.min`, `float.mid`, `float.max`, `float.custom(bits, mode)` | Approximatif ; interdit pour clés/invariants exacts. |

Règle structurante :

```text
decimal = exact
float = approximatif contrôlé
```

| Type | Autorisé | Interdit |
|---|---|---|
| decimal | argent, comptabilité, quantité exacte, taxe, taux contractuel exact. | usage sans précision/scale explicite. |
| float | mesure, score, statistique, analytics, vectoriel. | clé, FK, invariant exact, argent, valeur de consensus transactionnel. |

## 5. Bool, texte et absence

### 5.1 Bool

`bool required` est la règle. Le bool nullable est interdit : il crée une fausse logique métier à trois états. Si trois états sont nécessaires, il faut un Enum explicite.

### 5.2 Texte

Tout texte doit déclarer :

```text
encoding
length policy
collation/comparison policy si nécessaire
normalisation éventuelle
```

Exemple de politique :

```text
Login utf8 length 3..64 compare case_insensitive normalized NFC required unique
DisplayName unicode length 1..128 compare culture_aware required
```

### 5.3 Absence typée

L’absence ne doit pas être un `NULL` ambiant. Elle doit être modélisée par un type optionnel ou une branche explicite.

```srpl
let maybeCustomer be optional one Customer
    from Customers
    where Customer.Id = CustomerId;

when maybeCustomer has value as Customer do
    return Customer;
otherwise
    fail CustomerNotFound "Customer does not exist";
end when;
```

Le cœur des prédicats doit rester booléen. L’absence ne doit pas contaminer les comparaisons.

## 6. Cardinalité explicite

Un résultat ne doit pas seulement porter un type. Il doit porter un type et une cardinalité attendue.

| Forme | Sens |
|---|---|
| `one Customer` | Exactement un élément. Erreur si zéro ou plusieurs. |
| `optional one Customer` | Zéro ou un élément. Traitement explicite obligatoire. |
| `many Customer` | Zéro à N éléments. |
| `nonempty many Customer` | Au moins un élément. |
| `collection of T` | Flux ou collection contractuelle de T. |

Cette discipline permet au compilateur et à l’optimiseur de produire des diagnostics plus utiles : singleton non garanti, cardinalité incompatible, agrégat vide, mutation trop large, `RowsAffected` inattendu.

## 7. Enum et flags

Un Enum est un type catalogué, pas une table cachée.

```srpl
enum Role : u16
{
    User = 1 alias "Utilisateur";
    Admin = 2 alias "Administrateur";
}

enum Permission : u64 flags
{
    Read = 1;
    Write = 2;
    Execute = 4;
    Admin = 8;
}
```

Règles :

| Règle | Justification |
|---|---|
| Valeurs explicites | Stabilité binaire et compatibilité. |
| Type sous-jacent explicite | Stockage compact et sérialisation stable. |
| Alias séparé | Présentation sans changer la valeur. |
| Flags validés | Interdire les bits inconnus. |
| Versionnement | Compatibilité clients et contrats. |

## 8. Tables

Une Table est une relation persistante typée.

```srpl
table Security.User
{
    Id i64 primary key;
    Login utf8 required unique;
    Role Role required;
    Permissions Permission flags required;
    IsActive bool required;
    CreatedAt timestamp required;
}
```

Une Table doit porter :

```text
Columns[]
Constraints[]
AccessPaths[]
StoragePolicy
StatisticsPolicy
CurrentStatsVersion
RowCountExactAtSnapshot
```

Les Tables ne sont pas appelables. Elles sont accessibles uniquement par Procedure autorisée.

## 9. StructuredObjects

Un StructuredObject est un shape tabulaire typé utilisé comme paramètre, retour, batch d’import ou résultat intermédiaire contractuel.

### 9.1 StructuredObject générique

```srpl
structured ImportUserLine
{
    Login utf8;
    DisplayName utf8;
    RoleName utf8;
}
```

### 9.2 StructuredObject relationnel

```srpl
structured OrderLineInput
{
    ProductId i64 references Inventory.Product.Id;
    Quantity i32;
}
unique by ProductId;
```

Règle centrale :

```text
StructuredObject = shape + invariants intrinsèques
Procedure = invariants contextuels
```

Exemple : `Quantity > 0` peut être intrinsèque à `OrderLineInput`, mais `ProductId must be sellable in Country X` est contextuel et relève de la Procedure.

### 9.3 Cardinalité protocolaire

Un StructuredObject transporté par RPC doit inclure :

```text
Name
ContractHash
RowCountExact
ColumnCount
Column descriptors
Layout
PayloadLength
Batch descriptors
```

Layouts possibles :

| Layout | Usage |
|---|---|
| RowMajor | Petits objets, OLTP, validation ligne. |
| ColumnMajor | Gros objets, analytics, agrégations, GPU batch. |
| Hybrid | Paramètres mixtes ou retour multi-usage. |

Classes utiles : `Small`, `Medium`, `Large`, `Skewed`, `Unique`, `Duplicated`, `Sorted`.

## 10. SRPL : rôle et identité

SRPL signifie **Strict Relational Procedure Language**. Ce n’est pas un SQL renommé. C’est un langage de procédures relationnelles transactionnelles, déclaratif, typé et borné.

SRPL doit :

| Objectif | Implication |
|---|---|
| Lire comme une intention | Verbes canoniques, syntaxe stable, phrases courtes. |
| Compiler vers IR | AST typé puis IR relationnel/sémantique. |
| Protéger la transaction | Transaction implicite, erreurs contrôlées, rollback/poison. |
| Rester relationnel | Priorité aux opérations ensemblistes. |
| Éviter l’opacité | Boucles bornées uniquement, pas de réseau/fichier externe. |
| Stabiliser les plans | Contrats, cardinalités, shapes et versions. |

## 11. Structure canonique d’une Procedure

Forme V0 consolidée :

```srpl
procedure Namespace.Name
accepts
    Param1 Type1,
    Param2 Type2
returns
    ResultName collection of ResultType
begin
    ensure condition
        else fail ErrorCode "Message";

    let intermediate = ...;

    update Table
        set Column = Expression
        where Predicate;

    ensure affected rows equals ExpectedCount
        else fail ErrorCode "Message";

    return ResultName from
        select ...;
end;
```

Une variante plus explicite, inspirée du PDF SRPL, peut déclarer les capacités :

```srpl
transaction procedure BuildCustomerBalanceSummary
takes
    CustomerIds as many CustomerId
reads
    Customers as set of Customer,
    Invoices  as set of Invoice
returns
    many CustomerBalanceSummary
with
    isolation as serializable,
    access mode as read only,
    semantics as set,
    missing values as explicit
do
    let selectedCustomers be many Customer
        from Customers
        where Customer.Id is any of CustomerIds;

    let summaries be many CustomerBalanceSummary
        from selectedCustomers as Customer
        match Invoices as Invoice
            by Customer.Id = Invoice.CustomerId
        group by Customer.Id, Customer.Name
        compute
            CustomerId      as Customer.Id,
            CustomerName    as Customer.Name,
            InvoiceCount    as count of Invoice,
            TotalOpenAmount as sum of Invoice.OpenAmount else 0;

    return summaries;
end procedure;
```

La syntaxe finale reste ouverte, mais la sémantique cible est stable.

## 12. Exemple canonique : réservation de stock

```srpl
procedure Inventory.ReserveStock
accepts
    ProductId i64,
    Quantity i32
returns
    Reservation collection of ReservationResult
begin
    ensure Quantity > 0
        else fail InvalidQuantity "Quantity must be greater than zero";

    ensure exists Product where Id = ProductId
        else fail ProductNotFound "Product does not exist";

    update Stock
        set AvailableQuantity = AvailableQuantity - Quantity
        where ProductId = ProductId
          and AvailableQuantity >= Quantity;

    ensure affected rows equals 1
        else fail InsufficientStock "Not enough stock";

    return Reservation from
        select ProductId,
               Quantity,
               current transaction timestamp as ReservedAt;
end;
```

Points importants :

| Élément | Rôle |
|---|---|
| `ensure Quantity > 0` | Invariant d’entrée. |
| `exists Product` | Validation de référence métier. |
| `update ... where ... AvailableQuantity >= Quantity` | Mutation atomique conditionnelle. |
| `affected rows equals 1` | Contrôle de cardinalité de mutation. |
| `current transaction timestamp` | Temps déterministe attaché à la transaction. |

## 13. Transaction implicite

L’utilisateur n’écrit pas `transaction required`. Toute Procedure est transactionnelle par essence.

```text
Invocation
  -> TransactionScope
  -> TxId
  -> MVCC visibility
  -> WAL coverage
  -> Commit/Rollback
```

Le contrat peut déclarer les capacités ou politiques : read only/read write, isolation, budgets, objets lus/écrits. Mais l’existence d’une transaction ne doit pas dépendre d’une commande optionnelle.

## 14. Gestion d’erreur

SRPL doit distinguer : erreur métier attendue, erreur de contrat, erreur système, poison transactionnel.

```srpl
begin
    try
        ...
    catch InvalidQuantity
        rollback;
        return error InvalidQuantity;
    catch
        poison;
        rollback;
        return error UnexpectedFailure;
end;
```

États internes pertinents :

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

Un état `Poisoned` signifie que la transaction ou l’invocation a rencontré une condition qui interdit une continuation normale.

## 15. Temps et déterminisme

`current timestamp` brut est dangereux parce qu’il dépend du runtime et de l’instant d’évaluation. SRPL doit exposer des temps produits par le moteur :

| Expression | Sens |
|---|---|
| `current transaction timestamp` | Timestamp stable rattaché à la transaction. |
| `current invocation timestamp` | Timestamp stable rattaché à l’appel. |
| `current monotonic epoch` | Compteur monotone moteur, utile pour ordering interne. |

Ces valeurs doivent être traçables et rejouables dans les limites nécessaires au recovery et à l’audit.

## 16. Mutations

SRPL autorise `insert`, `update`, `delete`, `merge` sous contrat.

Règles V0 :

| Mutation | Règle |
|---|---|
| Insert | Contrôle contraintes + WAL. |
| Update | `where` obligatoire pour mutation multi-lignes. |
| Delete | `where` obligatoire pour mutation multi-lignes. |
| Merge | Contrat cardinal obligatoire. |
| Toutes | `RowsAffected` disponible. |
| Toutes | Couverture WAL obligatoire. |

Les mutations non bornées doivent être refusées ou nécessiter une annotation explicite de maintenance avec permissions admin.

## 17. Lecture, ordre et résultat

Les résultats sont des `ResultStream` typés. Les metadata précèdent le payload.

Règle d’ordre :

```text
Sans order by : l’ordre n’est pas contractuel.
Avec order by : l’ordre devient contractuel.
```

Une Procedure qui promet une séquence ordonnée doit le déclarer. Une relation n’est pas une liste.

## 18. Boucles et ensemblisme

SRPL privilégie l’ensembliste. Les boucles générales non bornées sont interdites dans le noyau.

Autorisées :

```srpl
for each Line in RequestedLines do
    ensure Line.Quantity > 0
        else fail InvalidQuantity;
end for;

repeat 3 times do
    collect diagnostics;
end repeat;
```

Interdites dans le noyau :

```text
while non borné
goto
récursion libre
réseau externe
filesystem externe
random non déterministe
SQL dynamique textuel
```

## 19. Compilation SRPL

Chaîne cible :

```text
SRPL source
  -> Lexer/Parser
  -> AST typé
  -> Binding noms/types/cardinalités
  -> Validation sémantique
  -> IR relationnel / ALT sémantique
  -> Procedure Plan candidates
  -> Costing + evidence
  -> Procedure Plan Cache
  -> Procedure Code Cache optionnel
```

Aucun code natif ne doit être produit sans :

```text
ContractHash
CatalogVersion
StatsVersion
PlanId
PolicyVersion
DecisionTraceId
```

## 20. Contrat de Procedure

Structure conceptuelle :

```text
ProcedureContract {
    ProcedureId,
    QualifiedName,
    ContractHash,
    InputShape,
    StructuredObjectContracts[],
    OutputShape,
    ResultStreamMetadata,
    RequiredPermissions[],
    ReadSetDeclared[],
    WriteSetDeclared[],
    IsolationPolicy,
    ResourcePolicy,
    ProtocolLayout,
    CompatibilityPolicy
}
```

Le contrat sert à :

| Usage | Impact |
|---|---|
| RPC | Vérifier paramètres et shape. |
| Sécurité | Calculer permissions nécessaires. |
| Optimizer | Spécialiser par types/cardinalités. |
| Plan Cache | Indexer et invalider les plans. |
| Audit | Comprendre ce qui a été demandé. |
| Client SDK | Générer bindings typés. |

## 21. Set semantics par défaut

SRPL doit privilégier la sémantique d’ensemble. Les doublons ne doivent pas être un comportement implicite comme dans le SQL en sacs.

| Modèle | Position SRPL |
|---|---|
| Ensemble | Défaut du noyau relationnel. |
| Bag/multiset | À introduire explicitement si requis. |
| Sequence | Vue ordonnée explicite, jamais implicite. |
| Stream | Forme de transport/runtime, pas sémantique relationnelle par défaut. |

Cette décision simplifie les preuves, les réécritures algébriques, la compréhension des cardinalités et le modèle mental utilisateur.

## 22. Agrégats

Les agrégats doivent déclarer leur comportement sur ensemble vide.

```srpl
compute
    TotalAmount as sum of Order.Amount else 0,
    AverageScore as average of Score else absent,
    MaxAmount as max of Amount else absent,
    CountRows as count of Order;
```

Règles :

| Agrégat | Cas vide recommandé |
|---|---|
| `count` | 0. |
| `sum` | Exiger `else` explicite. |
| `average` | `absent` ou erreur explicite. |
| `min/max` | `absent` ou erreur explicite. |
| `list` | Exiger ordering si ordre contractuel. |

## 23. Maps : définition et rôle

Une Map est une projection matérialisée. Andromeda ne doit pas proposer de View virtuelle générique native.

```text
Map = définition relationnelle + stockage + statistiques + contrat + policy de cohérence
```

Types de Maps :

| Type | Usage |
|---|---|
| Projection Map | Projection simple ou filtre matérialisé. |
| Join Map | Jointure matérialisée. |
| Aggregate Map | Agrégation maintenue. |
| Analytical Map | Projection large pour analytique. |
| Security Map | Projection d’autorisation ou filtrage sécurité. |

## 24. Cohérence des Maps

Une Map doit refléter les données réelles ajoutées, modifiées, supprimées ou mergées selon sa policy.

| Mode | Sémantique | Usage recommandé |
|---|---|---|
| Immediate | Mise à jour dans la même transaction que la Table source. | Map simple, critique, coût faible. |
| Incremental | Delta journalisé puis appliqué contrôlé. | Agrégats, joins modérés. |
| Deferred | Refresh différé par job. | Analytics non critiques. |
| SnapshotOnly | Cohérence avec snapshot publié. | Analytique large, reporting. |

Règles V0 :

```text
Map simple sans agrégat : Immediate possible.
Map avec agrégat : Incremental recommandé.
Map analytique large : Deferred ou SnapshotOnly.
Map utilisée par Procedure critique : policy explicite obligatoire.
```

## 25. Exemple Map

```srpl
map Sales.CustomerMonthlyAmount
from Sales.Order
join Sales.Customer on Customer.Id = Order.CustomerId
group by Customer.Id, month(Order.CreatedAt)
select
    Customer.Id as CustomerId,
    month(Order.CreatedAt) as Month,
    sum(Order.Amount) as TotalAmount
refresh incremental
storage columnar
statistics enabled;
```

### 25.1 Transaction Map Immediate

```text
TransactionScope
  -> mutation Table
  -> delta Map
  -> WAL records Table + Map
  -> commit atomique
```

### 25.2 Map Incremental

```text
Mutation source
  -> WAL source
  -> MapDeltaLog
  -> Apply controlled delta
  -> validate Map state
  -> publish Map version/statistics
```

## 26. Maps et analytics

Les Maps portent :

```text
histogrammes
cardinalité
skew
compression
access paths
column segments
snapshots analytiques
éventuel GPU batch hors commit path
```

Une Map analytique peut être columnar. Mais si elle se trouve sur le chemin de commit OLTP, son coût doit être strictement borné.

## 27. Diagnostics SRPL souhaités

| Diagnostic | Exemple |
|---|---|
| Cardinalité non garantie | `one Customer` mais prédicat non unique. |
| Mutation trop large | `update` sans `where` ou sans budget. |
| Absence non traitée | `optional one` utilisé comme `one`. |
| Float interdit | `float` utilisé comme clé. |
| Temps non déterministe | `current timestamp` brut. |
| Boucle non bornée | `while` sans limite. |
| Contrat incompatible | `StructuredObject` version client obsolète. |
| Map trop coûteuse | `Immediate` sur join/aggregate large. |
| Sortie non ordonnée | ResultStream déclaré ordered sans `order by`. |

## 28. Synthèse du document

SRPL et le Type System sont les instruments principaux de réduction de l’ambiguïté. L’objectif n’est pas d’être plus permissif que SQL ; l’objectif est d’être plus sûr, plus typé, plus explicable, plus compilable et plus stable sous charge.

La bonne règle de conception est :

```text
Ce qui est implicite dans SQL doit devenir explicite dans SRPL
si cela affecte la sécurité, la cardinalité, la cohérence, le coût ou le recovery.
```



## 29. Mini-grammaire SRPL V0 indicative

Cette grammaire n’est pas normative, mais elle clarifie la direction : syntaxe réduite, verbes stables, cardinalité explicite et absence d’ambiguïté textuelle.

```text
procedure_decl := "procedure" qualified_name accepts? returns? "begin" stmt* "end" ";"?
accepts        := "accepts" param_decl ("," param_decl)*
returns        := "returns" result_decl
param_decl     := identifier type_ref cardinality?
result_decl    := identifier result_shape
stmt           := ensure_stmt | let_stmt | mutation_stmt | return_stmt | try_stmt | for_each_stmt | fail_stmt
ensure_stmt    := "ensure" predicate "else" "fail" error_code string_literal
let_stmt       := "let" identifier "=" relation_expr ";"
mutation_stmt  := insert_stmt | update_stmt | delete_stmt | merge_stmt
return_stmt    := "return" identifier "from" projection_expr ";"
cardinality    := "one" | "optional one" | "many" | "nonempty many" | "collection of"
```

Décisions à préserver : pas de noms implicites, pas de colonnes positionnelles, pas de `SELECT *`, pas de SQL dynamique textuel.

## 30. Compatibilité de contrat

Une Procedure doit évoluer sans casser silencieusement les clients.

| Changement | Statut par défaut | Remarque |
|---|---|---|
| Ajouter un paramètre required | Breaking | Le client existant ne peut pas appeler. |
| Ajouter un paramètre optional avec défaut explicite | Additive | Compatible si défaut stable. |
| Changer type de paramètre | Breaking | Sauf widening explicitement validé. |
| Changer cardinalité de retour many -> one | Breaking | Risque runtime majeur. |
| Ajouter colonne de résultat en fin avec policy additive | Additive possible | Si clients nommés et non positionnels. |
| Renommer colonne de résultat | Breaking | Le contrat change. |
| Ajouter error code documenté | Additive contrôlée | Le client doit pouvoir recevoir erreur inconnue. |
| Changer permissions requises | Security-impact | Audit et version policy requis. |
| Changer isolation | Behavior-impact | Peut être breaking pour latence/anomalies. |

Le `ContractHash` doit inclure les éléments qui affectent la compatibilité observable.

## 31. Modèle d’erreur SRPL

| Famille | Exemple | Traitement |
|---|---|---|
| BusinessError | `InsufficientStock`, `CustomerNotFound`. | Retour typé, rollback si mutation en cours. |
| ContractError | Paramètre manquant, StructuredObject incompatible. | Rejet avant transaction active si possible. |
| PermissionError | Principal non autorisé. | Rejet + audit sécurité. |
| CardinalityError | `one` retourne plusieurs lignes. | Fail déterministe. |
| ConstraintError | Unique/FK/check violation. | Fail + rollback. |
| ResourceError | Timeout, TempBytes quota. | Fail contrôlé, rollback. |
| SystemError | I/O, corruption, panic évité. | Poison + rollback/recovery path. |

Une erreur ne doit pas être seulement une chaîne. Elle doit avoir un code, une famille, une criticité, une trace et une politique de retry.

## 32. Invariants déclaratifs

Les invariants doivent être placés au bon niveau.

| Niveau | Invariant | Exemple |
|---|---|---|
| Type | Format intrinsèque. | `EmailAddress` normalisé. |
| Enum | Valeurs autorisées. | `OrderStatus`. |
| StructuredObject | Shape et règles internes. | `unique by ProductId`. |
| Table | Clés, FK, checks. | `Stock.AvailableQuantity >= 0`. |
| Procedure | Règle contextuelle. | Réserver seulement si produit vendable. |
| Map | Cohérence projection. | Refresh incremental validé. |
| Policy | Autorisation/ressource. | Max rows returned. |

Bonne règle : mettre un invariant au niveau le plus bas où il reste vrai sans contexte applicatif supplémentaire.

## 33. Exemple de Procedure batchée avec StructuredObject

```srpl
procedure Sales.CreateOrder
accepts
    CustomerId i64,
    Lines OrderLineInput collection
returns
    CreatedOrder collection of CreateOrderResult
begin
    ensure rowcount(Lines) > 0
        else fail EmptyOrder "Order must contain at least one line";

    ensure all Lines satisfy Quantity > 0
        else fail InvalidQuantity "Every line quantity must be positive";

    ensure all Product exists in Inventory.Product
        matching Lines.ProductId = Product.Id
        else fail ProductNotFound "One or more products do not exist";

    insert into Sales.Order
        values new Order(CustomerId = CustomerId,
                         Status = OrderStatus.Created,
                         CreatedAt = current transaction timestamp);

    insert into Sales.OrderLine
        from Lines
        select current order id, ProductId, Quantity;

    update Inventory.Stock
        from Lines
        set AvailableQuantity = AvailableQuantity - Lines.Quantity
        where Stock.ProductId = Lines.ProductId
          and Stock.AvailableQuantity >= Lines.Quantity;

    ensure affected rows equals rowcount(Lines)
        else fail InsufficientStock "At least one product has insufficient stock";

    return CreatedOrder from
        select current order id as OrderId,
               current transaction timestamp as CreatedAt;
end;
```

Cet exemple force les points importants : cardinalité du StructuredObject, validation ensembliste, mutation conditionnelle, contrôle `RowsAffected`, temps transactionnel.

## 34. Règles anti-dynamisme

| Interdit | Alternative SRPL |
|---|---|
| Construire un nom de table par concaténation. | Procedure distincte ou Map/catalog contractuel. |
| Construire un prédicat texte. | Paramètres typés et prédicats déclarés. |
| Retourner colonnes variables selon branche. | Union discriminée ou résultats nommés distincts. |
| Utiliser `NULL` comme état métier. | Optional ou Enum d’état. |
| Dépendre de l’ordre physique. | `order by` contractuel. |
| Boucler jusqu’à condition non bornée. | Itération sur collection finie ou job admin borné. |

## 35. Recommandations pour le compilateur

Le compilateur SRPL doit produire au moins quatre niveaux de diagnostic :

| Niveau | Sens |
|---|---|
| Error | Compilation impossible ou contrat invalide. |
| Warning | Possible ambiguïté ou coût élevé. |
| Advisory | Suggestion d’index, Map, stats ou rewrite. |
| Trace | Décisions de binding et normalisation. |

Les diagnostics doivent être stables et référencés par codes : `SRPL-CARD-001`, `SRPL-TYPE-014`, `SRPL-MAP-022`, etc. Cela permettra d’outiller IDE, CI/CD, DefinitionBatch DryRun et documentation.


---

## Annexe locale — règle de consolidation

Cette version consolide les fichiers projet et les trois PDF de fondation sans tenter de transformer Andromeda en moteur SQL généraliste. Les équivalences SQL restent pédagogiques. La surface native reste :

```text
QUIC + RPC custom + Procedure cataloguée + SRPL + contrats typés + WAL/MVCC/recovery
```

Toute extension future doit rester définissable, déterministe ou explicitement bornée, typée, observable, récupérable après crash, versionnée, explicable et désactivable.

