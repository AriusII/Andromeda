# P03 Catalog Store Durable Evidence

Date: 2026-05-10

## Objective

Retain reproducible evidence that catalog publication is not visible until durable commit evidence exists. This source supports the P03 exit criterion: every catalog publication has durable evidence or is rejected. The durable envelope is the `CatalogMutationRecord` Begin/Apply/Commit sequence.

## Code Evidence

| Area | Evidence |
| --- | --- |
| Durable receipt type | `crates/andromeda-catalog-store/src/publication_receipt.rs:27` defines `CatalogMutationDurability::StorageWal { commit_lsn, durable_lsn }` and `ExternalMarker`; `validate()` rejects zero commit LSN, stale durable LSN, and zero marker evidence. |
| Receipt construction | `crates/andromeda-catalog-store/src/publication_receipt.rs:150` builds `CatalogPublicationReceipt` only after `CatalogPublicationCommitEvidence::validate_for_publication_plan`. |
| WAL before visible publication | `crates/andromeda-catalog/tests/catalog_store_contract/mutation_wal.rs:275` proves `apply_definition_batch_durably` appends Begin/Apply/Commit, flushes through the commit LSN, and only then exposes a durable snapshot receipt. |
| Failed append remains unpublished | `crates/andromeda-catalog/tests/catalog_store_contract/mutation_wal.rs:320` proves a commit append failure leaves the snapshot unpublished and object count unchanged. |
| Flush evidence below commit LSN rejected | `crates/andromeda-catalog/tests/catalog_store_contract/mutation_wal.rs:364` proves stale durable LSN evidence is rejected without publication. |
| External marker path | `crates/andromeda-catalog/tests/catalog_store_contract/mutation_wal.rs:517` proves durable publication can use an external nonzero marker when no storage LSN is carried. |
| Invalid publication evidence rejected | `crates/andromeda-catalog/tests/catalog_store_contract/mutation_wal.rs:542` rejects uncommitted evidence and non-durable LSN evidence. |
| DefinitionBatch hashes in durable report | `crates/andromeda-catalog/tests/catalog_store_contract/mutation_wal.rs:629` proves durable apply report and receipt carry nonzero `DefinitionBatchSourceHash` and `DefinitionBatchDependencyGraphHash`. |

## Executable Commands

```powershell
cargo test -p andromeda-catalog --test catalog_store_contract --locked -- --nocapture
cargo test -p andromeda-catalog-recovery --test catalog_publication_subscription_runtime_contract --locked -- --nocapture
```

Person 14 ran:

```powershell
cargo test -p andromeda-catalog-recovery --test catalog_publication_subscription_runtime_contract --locked -- --nocapture
```

Result: passed, 10 tests.

## Release Gate

Go only if durable publication receipts carry either a nonzero durable LSN that reaches the commit LSN or a nonzero external durability marker. Any missing, zero, stale, or mismatched evidence remains a Catalog error before visible publication.
