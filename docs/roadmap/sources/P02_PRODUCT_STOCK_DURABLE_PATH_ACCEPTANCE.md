# P02 ProductStock Durable Path Acceptance

## Objective

Define the acceptance evidence for the P02 `Inventory.ReserveStock` / `Inventory.ProductStock` durable vertical path.

This document is not a production-readiness claim. It records the local V0 evidence that the demo vertical publishes ProductStock state only after durable WAL commit evidence exists.

## Scope

Owned vertical:

- Procedure: `Inventory.ReserveStock`
- Storage object: `Inventory.ProductStock`
- Result stream: V0 metadata, one batch, terminal completion
- Durability rule: WAL commit LSN must exist before ProductStock publication
- Audit rule: pre-transaction rejection evidence must not fabricate transaction or durable LSN correlation

Out of scope:

- Production gateway readiness
- Multi-node HA/DR proof
- Backup/PITR release evidence
- Checker changes

## Acceptance Matrix

| Requirement | Evidence | Reject Condition |
|---|---|---|
| ProductStock publication waits for durable commit evidence | `product_stock_path::v0_inventory_heap_product_stock_store_publishes_only_after_durable_commit` asserts ProductStock visible row, published commit, durable commit LSN, active heap slot count, redo payload, and WAL record order. | Visible ProductStock changes before commit LSN, missing redo evidence, or mismatch between ProductStock commit and vertical completion. |
| ProductStock durable evidence is internally coherent | `product_stock_path` rejects `durable_commit_lsn <= redo_record_lsn`, mismatched redo binding, non-advancing PageLSN, and HREDOV1 payload mismatch. | Any incoherent proof publishes ProductStock or advances PageLSN. |
| Commit append failure does not publish ProductStock | `product_stock_path::v0_inventory_product_stock_adapter_aborts_prepared_heap_state_when_commit_evidence_is_absent` injects commit append failure and verifies `publish_count == 0`, abort happened, and visible stock is unchanged. | ProductStock publish count increments after failed commit append, prepared state remains live, or WAL contains a commit record. |
| Contract rejection does not touch ProductStock | `product_stock_path::v0_inventory_product_stock_adapter_is_not_touched_for_contract_rejection` verifies WAL is empty and ProductStock counters are untouched. | ProductStock prepare/publish/abort runs after pre-transaction contract rejection. |
| Durable redo binds heap state | `wal_recovery_evidence::v0_inventory_file_wal_replays_exec_product_stock_hredov1_into_context` decodes durable HREDOV1 and replays it into recovery context. | Redo payload cannot reconstruct expected row or replay applies non-committed records. |
| ResultStream completion carries durable evidence | `result_stream_metadata::v0_inventory_result_stream_metadata_precedes_batch_and_typed_completion` decodes the V0 completion payload and asserts `durable_lsn == 3` and non-zero. | Completion is emitted without non-zero durable LSN payload evidence. |

## Evidence Command

```powershell
cargo test -p andromeda-inventory-demo --test v0_vertical_e2e --locked -- --nocapture
```

Local result on 2026-05-10: PASS, 25 tests passed.

## Decision

P02 durable vertical evidence is accepted for the local V0 demo path when the focused command above passes. Release readiness remains blocked by the broader retained evidence packet required by the roadmap gates.
