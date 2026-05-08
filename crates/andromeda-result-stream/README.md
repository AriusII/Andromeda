# andromeda-result-stream

## Purpose

`andromeda-result-stream` owns execution result sequencing outside `andromeda-exec`.

## Scope

The crate enforces metadata-before-payload, cardinality row-count contracts, bounded stream capacity, and durable terminal completion evidence. It does not own QUIC transport, protobuf byte formats, procedure contracts, WAL records, storage truth, or business-specific result policy.

## Validation

```powershell
cargo check -p andromeda-result-stream --all-targets
```
