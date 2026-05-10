# P03 ContractHash Canonicalization Evidence

Date: 2026-05-10

## Objective

Retain evidence that `ContractHash` is canonical procedure shape evidence, not raw SRPL source evidence and not catalog publication evidence. P03 requires the hash to change only when the observable contract changes.

## Code Evidence

| Area | Evidence |
| --- | --- |
| Canonical hash implementation | `crates/andromeda-procedure-contract/src/hash.rs:15` and `:33` compute field-tagged canonical procedure hashes over materialized contract fields. |
| Canonical validation | `crates/andromeda-procedure-contract/src/materialization.rs:102` rejects stored `contract_hash` values that do not match `canonical_hash()`. |
| Materialization path | `crates/andromeda-procedure-contract/src/materialization.rs:145` materializes candidates with canonical hash evidence. |
| Digest determinism | `crates/andromeda-procedure-contract/tests/procedure_contract_digest.rs:117` proves deterministic digest-backed hashing and avalanche behavior. |
| Formatting independence | `crates/andromeda-procedure-contract/tests/procedure_contract_digest.rs:141` proves raw SRPL byte hashes differ for formatting-only changes while canonical `ContractHash` stays stable. |
| Observable shape changes | `crates/andromeda-procedure-contract/tests/procedure_contract_digest.rs:141` also proves result growth and input type drift change `ContractHash`. |
| CatalogVersion exclusion | `crates/andromeda-procedure-contract/tests/procedure_contract_digest.rs:235` proves `CatalogVersion` is carried by binding evidence and not included in canonical procedure shape. |
| Compatibility policy | `crates/andromeda-procedure-contract/tests/procedure_contract_digest.rs:338` and `:382` prove exact-hash and additive compatibility gates observe the canonical hash and shape diagnostics. |
| Stale hash rejection | `crates/andromeda-procedure-contract/tests/procedure_contract_digest.rs:552` proves nonzero binding evidence is insufficient when the stored hash is not canonical. |
| Catalog source drift | `crates/andromeda-catalog/tests/catalog_digest_contract.rs:189`, `:211`, `:222`, and `:257` separate canonical contract shape from ProcedureId and CatalogVersion drift captured by DefinitionBatch source evidence. |

## Executable Commands

```powershell
cargo test -p andromeda-procedure-contract --test procedure_contract_digest --locked -- --nocapture
cargo test -p andromeda-catalog --test catalog_digest_contract --locked -- --nocapture
```

Related publication validation run by Person 14:

```powershell
cargo test -p andromeda-catalog-recovery --test catalog_publication_subscription_runtime_contract --locked -- --nocapture
```

Result: passed, 10 tests.

## Release Gate

Go only if invocation and publication evidence keep these identities separate: `ContractHash` for observable procedure shape, `CatalogVersion` for publication version, `DefinitionBatchSourceHash` for ordered source identity, and dependency graph hash for canonical dependency evidence.
