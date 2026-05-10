# Andromeda glossary

> **Status:** Terminology reference  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define native Andromeda terms.
- Provide SQL-adjacent educational equivalents without making them design drivers.
- Preserve consistent vocabulary across docs.

## Native structural terms

| Term | Definition | Educational equivalent |
|---|---|---|
| Instance | Running Andromeda engine with surfaces and databases. | Server instance. |
| System Database | C5 database containing registries, policies, certificates, hardware profiles, and audit. | Master/system catalog. |
| Database | Isolated user database. | Database. |
| Namespace | Logical name boundary for tables, maps, enums, structured objects, and procedures. | Schema. |
| Table | Persistent typed relation. | Table. |
| Column | Typed relation attribute. | Column. |
| Row | Tuple instance in a relation. | Row/tuple. |
| Map | Stored materialized projection with a consistency policy. | Materialized view. |
| Enum | Cataloged named discrete type. | Enum/reference type. |
| StructuredObject | Typed tabular input or output shape. | TVP or tabular record. |
| Procedure | Cataloged transactional behavior unit callable by RPC. | Stored procedure. |
| DefinitionBatch | Validated ordered catalog change batch. | Controlled migration batch. |
| Modelization | External design artifact imported through DefinitionBatch. | Modeling artifact. |

## Runtime terms

| Term | Definition |
|---|---|
| Invocation | Effective execution of a Procedure. |
| Procedure Store | Historical store of invocations, plans, costs, outcomes, and runtime evidence. |
| Procedure Plan | Physical execution plan for a Procedure. |
| Procedure Plan Cache | Bounded cache of active Procedure plans. |
| ResultStream | Typed and metadated output stream. |
| ContractHash | Canonical hash of a Procedure contract. |
| CatalogVersion | Published catalog version. |
| StatsVersion | Published statistics version. |
| PolicyVersion | Published policy version. |
| DecisionTrace | Structured explanation of a runtime decision. |
| RecoveryReport | Structured evidence produced after recovery. |
| ScenarioEvidence | Non-authoritative benchmark or predictive evidence. |

## Terms to avoid

| Avoid | Prefer | Reason |
|---|---|---|
| Query | Procedure or Invocation | Application surface is not ad hoc query text. |
| Query Store | Procedure Store | Runtime evidence is Procedure-centric. |
| View | Map | Native projections are materialized and policy-bound. |
| Migration script | DefinitionBatch | Catalog evolution is validated and transactional. |
| Model | Modelization | The model is an external design artifact, not runtime truth. |
| SQL ad hoc | Not native | Mention only as a comparison. |

## Naming rule

Use the Andromeda term first. Use an educational equivalent only when explaining the concept to readers familiar with SQL Server, Oracle, PostgreSQL, or other RDBMS engines.
