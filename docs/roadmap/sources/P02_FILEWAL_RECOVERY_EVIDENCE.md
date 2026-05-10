# P02 FileWAL Recovery Evidence

## Objective

Record the FileWAL recovery evidence for the P02 durable `Inventory.ReserveStock` / `ProductStock` path.

## Recovery Contract

The P02 V0 path writes:

- `TxBegin` at LSN 1
- durable ProductStock heap row redo at LSN 2
- `TxCommit` at LSN 3

Recovery must replay only committed redo records. Non-redo transaction markers must be skipped, and incomplete or uncommitted records must not publish ProductStock state.

## Evidence Matrix

| Requirement | Evidence | Expected Result |
|---|---|---|
| FileWAL reports durable prefix after sync | `wal_recovery_evidence::v0_inventory_file_wal_recovers_only_committed_redo_after_sync` | `durable_lsn == 3` |
| Recovery plan replays only committed redo | `wal_recovery_evidence::v0_inventory_file_wal_recovers_only_committed_redo_after_sync` | committed replay LSNs are `[2]` |
| Transaction markers are not replayed as heap redo | `wal_recovery_evidence::v0_inventory_file_wal_recovers_only_committed_redo_after_sync` | LSN 1 and LSN 3 decisions are `SkipNonRedoRecord` |
| Durable HREDOV1 payload reconstructs ProductStock row | `wal_recovery_evidence::v0_inventory_file_wal_replays_exec_product_stock_hredov1_into_context` | decoded row is `ProductId=42`, `AvailableQuantity=7` |
| Redo plan creates one recovered heap page | `wal_recovery_evidence::v0_inventory_file_wal_replays_exec_product_stock_hredov1_into_context` | one redo-created page, page LSN equals redo record LSN |
| Replay remains clean | `wal_recovery_evidence::v0_inventory_file_wal_replays_exec_product_stock_hredov1_into_context` | `has_replay_errors == false` |
| Crash before client ACK does not lose durable ProductStock redo | `wal_recovery_evidence::v0_inventory_file_wal_recovers_product_stock_after_crash_before_client_ack` | reopened FileWal has durable `TxBegin` / `RowInsert` / `TxCommit`; recovery applies only committed HREDOV1 redo |
| Client ACK and ResultStream frames are non-authoritative | `wal_recovery_evidence::v0_inventory_file_wal_recovers_product_stock_after_crash_before_client_ack` | replay uses only durable FileWal records and ignores volatile result frames |

## Evidence Command

```powershell
cargo test -p andromeda-inventory-demo --test v0_vertical_e2e --locked -- --nocapture
```

Local result on 2026-05-10: PASS, 25 tests passed.

## Limits

This evidence proves local FileWAL recovery for the P02 V0 path. It does not replace backup/PITR, HA/DR, multi-process crash injection, or retained release evidence.
