# Phase 0 Contract Test Vectors

## Purpose

This file records the executable vectors represented in the Rust unit tests. The vectors are intentionally
small and deterministic so later binary, Protobuf, and recovery tests can replace them with serialized
golden fixtures.

## Vectors

| Surface           | Vector                                            | Expected result                                        |
|-------------------|---------------------------------------------------|--------------------------------------------------------|
| ContractHash      | 31 bytes                                          | Reject as `Contract` error.                            |
| ContractHash      | 32 identical bytes                                | Accept as a test vector hash.                          |
| PayloadKind       | Numeric values 1 through 9                        | Map to HELLO through ERROR families.                   |
| PayloadKind       | Numeric value 10                                  | Reject as `Protocol` error.                            |
| FrameEnvelope     | RPC request with zero ContractHash                | Reject before transaction creation.                    |
| FrameEnvelope     | HELLO with zero ContractHash                      | Accept before contract binding.                        |
| FrameHeader       | RPC batch on command stream                       | Reject as `Protocol` error.                            |
| QUIC DATAGRAM     | RPC batch                                         | Reject; only soft telemetry may use datagram behavior. |
| DefinitionBatch   | One valid table create at CatalogVersion 10       | Dry-run produces CatalogVersion 11.                    |
| ProcedureContract | Zero ContractHash                                 | Reject as `Contract` error.                            |
| SRPL source       | `while true` or `execute sql`                     | Reject as forbidden core construct.                    |
| Result metadata   | Cardinality `One` without RowCountExact           | Reject as `Contract` error.                            |
| Transaction       | Active to durable WAL flush                       | Reject because commit was not requested.               |
| Transaction       | Committing without durable LSN                    | Reject visible commit.                                 |
| Transaction       | Committing with durable LSN 42                    | Allow committed visible state.                         |
| MVCC row          | BeginTs 10, EndTs 20                              | Visible at 10 and 19, invisible at 9 and 20.           |
| WAL record        | RowInsert without TransactionId                   | Reject as `Storage` error.                             |
| PageHeader        | Correct magic and format with nonzero CRC         | Accept.                                                |
| PageTrailer       | Nonzero payload CRC and torn-write guard          | Accept.                                                |
| Manifest          | Snapshot 3, checkpoint LSN 100, WAL start LSN 101 | Recovery plan replays from 101.                        |

## Phase 1 Local Prototype Vectors

| Surface | Vector                                                               | Expected result                                                                            |
|---------|----------------------------------------------------------------------|--------------------------------------------------------------------------------------------|
| Catalog | `inventory_reserve_stock_contract()`                                 | Valid contract with nonzero ContractHash and execute permission.                           |
| Proto   | `FrameEnvelope::rpc_execute_request` with nonzero ContractHash       | Valid RPC execute envelope.                                                                |
| Proto   | RPC execute envelope to completion/error                             | Related envelope keeps request, session, catalog, and contract binding.                    |
| QUIC    | `FrameBytes::validate` for execute frame on command stream           | Accept.                                                                                    |
| QUIC    | execute frame on result stream                                       | Reject as `Protocol` error.                                                                |
| Storage | `InMemoryWal` append TxBegin then TxCommit                           | LSNs are 1 then 2.                                                                         |
| Storage | Flush through TxBegin only                                           | Durable replay excludes unflushed TxCommit.                                                |
| Exec    | Contract mismatch before invocation                                  | Reject before any WAL record.                                                              |
| Exec    | `Inventory.ReserveStock` through `LocalVerticalRuntime<InMemoryWal>` | Commit with durable LSN 3 and three durable WAL records.                                   |
| CLI     | `andromeda-cli vertical`                                             | Prints committed status, rows affected, durable LSN, WAL record count, and contract trace. |
