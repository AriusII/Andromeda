# DEC-015 Inventory Business Procedure Slice

## Status

Accepted.

## Context

The local vertical prototype proves contract validation, authorization, WAL ordering, result metadata,
and completion mapping. It still used an opaque mutation payload for `Inventory.ReserveStock`, so the
demo could commit without proving domain rules such as positive quantity, matching product identity,
sufficient stock, or deterministic row-count evidence.

The next implementation step needs a small business procedure slice that advances domain logic without
expanding SRPL into ad hoc SQL or claiming production persistence.

## Decision

Add a typed `Inventory.ReserveStock` business slice in `andromeda-exec`:

1. `InventoryStock` models the visible stock input for one product.
2. `ReserveStockCommand` models the typed invocation intent.
3. `InventoryReserveStockExecutor` validates product identity, positive quantity, sufficient stock,
   and stock version shape.
4. `ReserveStockEffect` produces the next stock state, exact result evidence, deterministic mutation
   payload bytes, and a `LocalProcedure` adapter for the existing local vertical runtime.
5. The catalog fixture materializes `Inventory.ReserveStock` through `ProcedureContractCandidate` so
   its contract hash is canonical instead of a fixed test vector.
6. The catalog fixture also exposes the initial `Inventory.ProductStock`, `Inventory.Reservation`, and
   `Inventory.ReserveStock` definition batch for future catalog-bound execution.

This is an internal executable business slice. It is not an SRPL body specification, SQL endpoint, disk
durability claim, or network protocol change.

## Invariants Preserved

- Procedure-only execution remains the application boundary.
- No application-facing ad hoc SQL, gRPC, or runtime JSON default is introduced.
- Authorization still occurs before transaction creation in the local vertical runtime.
- Commit visibility still requires WAL flush through the commit LSN.
- The local prototype remains in-memory and does not claim production durability.
- Contract hashes for the fixture Procedure are canonical and validated against the contract shape.

## Validation

- `cargo fmt --all -- --check`
- `cargo check --workspace --quiet`
- `cargo test --workspace --quiet`
- `cargo run -q -p andromeda-cli -- vertical`
