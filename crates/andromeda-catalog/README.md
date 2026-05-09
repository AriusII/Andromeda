# andromeda-catalog

## Purpose

`andromeda-catalog` owns canonical catalog objects, object versions, Procedure contracts, DefinitionBatch behavior, catalog digests, publication/subscription evidence, procedure-store records, and catalog WAL-facing codecs.

The catalog is a durable control plane for typed Procedure contracts and object visibility. A catalog change must become visible only after durable mutation evidence exists and recovery can replay the committed catalog state.

## Scope

This crate owns:

- Catalog object descriptors, qualified names, dependency edges, snapshots, and version validation.
- Procedure contracts, contract hashes, compatibility policy, and temporary compatibility reexports from `andromeda-contract`.
- DefinitionBatch validation, dry-run behavior, migration planning, source hashes, dependency graph hashes, and catalog mutation records.
- Catalog recovery from durable mutation payloads and committed begin/apply/commit batch sequences.
- Catalog publication/subscription reports for Administration and HA/DR consumers, including durable LSN or durable marker evidence.
- Procedure Store runtime records and catalog-facing decision evidence.
- Catalog truth, publication, and version binding for catalog objects.
- Catalog-facing typed errors from `andromeda-error`, module-specific error enums, and `AndromedaResult` failures with stable error kinds.

## Non-goals

This crate does not own:

- Application-facing Procedure execution or dispatch.
- Storage WAL file ownership, page formats, heap redo, B+Tree formats, backup, restore, or startup policy.
- Transaction commit authority, MVCC visibility, lock management, or rollback execution.
- Application-facing ad hoc SQL, dynamic object names, dynamic predicates, or implicit null semantics in SRPL core work.
- Administration or HA/DR capabilities exposed through the Application Surface.
- Benchmark, GPU, analytics, learned-model output, statistics output, or plan-cache entries as catalog truth.

## Prerequisites

Before changing this crate, confirm that the change respects these requirements:

- Procedure contracts remain typed, versioned, hashable, and compatible under explicit policy.
- DefinitionBatch operations validate dependencies, source identity, and dependency graph identity before publication.
- Catalog mutations have durable begin/apply/commit evidence before the next catalog version is visible.
- Recovery reconstructs only fully committed batches with dense apply indexes and matching catalog identity.
- Publication reports include Administration or HA/DR audience only and carry durable WAL LSN evidence or a durable evidence marker.
- Procedure Store and statistics outputs are advisory unless a typed publication or runtime record makes their role explicit.
- Error paths use typed errors and stable error kinds for compatibility, validation, recovery, and publication failures.

## Procedure

1. Identify whether the change affects object modeling, contracts, DefinitionBatch, recovery, publication/subscription, or Procedure Store.
2. Preserve contract-first behavior. Do not introduce raw SQL, dynamic object shapes, or untyped Procedure return contracts.
3. Keep DefinitionBatch changes atomic and replayable. Update validation, dry-run behavior, digest inputs, durable mutation records, and recovery expectations together.
4. Gate visible catalog changes on durable evidence. A publication must prove durable WAL LSN coverage or an explicit durable marker before subscribers can acknowledge it.
5. Keep advisory evidence separate from truth. Plan-cache and statistics evidence belong to their owner crates unless catalog publication truth is directly involved.
6. Add tests for both acceptance and fail-closed behavior. Recovery and publication changes need committed, incomplete, duplicate, out-of-order, stale, and corruption-oriented cases.

## Validation

Recommended catalog gates:

```powershell
cargo test -p andromeda-catalog --test catalog_digest_contract -- --nocapture
cargo test -p andromeda-catalog --test batch_alter_drop_compat -- --nocapture
cargo test -p andromeda-catalog --test wal_record_design -- --nocapture
cargo test -p andromeda-catalog --test publication_subscription_recovery_contract -- --nocapture
cargo test -p andromeda-catalog --test catalog_publication_subscription -- --nocapture
cargo test -p andromeda-catalog --test procedure_store_contract -- --nocapture
```

For durable payload changes, add byte roundtrip, corruption rejection, property tests, fuzz coverage for malformed catalog payload bytes, and crash/recovery replay tests. For catalog-store or publication changes that cross into storage, also run the storage catalog WAL bridge and recovery integration tests.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| A DefinitionBatch is rejected | Inspect dependency ordering, base catalog version, source hash, dependency graph hash, object identity, and compatibility policy. |
| A catalog version is visible without subscriber acknowledgement | Verify publication report audience, durable LSN or marker evidence, replay expectation, and acknowledgement boundary. |
| Recovery skips a catalog mutation | Confirm the durable payload sequence contains begin, dense apply records, and commit for the same batch. |
| Procedure Store evidence appears authoritative unexpectedly | Check the evidence role and advisory boundary; feedback must remain explicitly classified. |

## References

- `src/lib.rs`
- `src/objects.rs`
- `src/contracts.rs`
- `src/batch/`
- `src/recovery.rs`
- `src/wal_record.rs`
- `src/publication_subscription/`
- `src/procedure_store/`
- `tests/catalog_digest_contract.rs`
- `tests/batch_alter_drop_compat.rs`
- `tests/wal_record_design.rs`
- `tests/publication_subscription_recovery_contract.rs`
- `tests/procedure_store_contract.rs`
