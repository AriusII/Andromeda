# Code Quality Register

## Baseline

Transient audit `.txt` artifacts were generated for the first cleanup pass and removed after review.

## Files Over 500 Lines

| File | Lines | Sprint target | Status |
|---|---:|---|---|
| `crates/andromeda-observe/src/events.rs` | 2006 | S2 | P0 split candidate; sink, decision, and correlation extracted |
| `crates/andromeda-observe/src/principal_binding.rs` | 670 | S3 | Split candidate |

## Wired Splits

| Facade | Submodules | Status |
|---|---|---|
| `crates/andromeda-catalog/src/batch.rs` | `definition`, `durability`, `mutation`, `plan` | Wired |
| `crates/andromeda-catalog/src/contracts.rs` | `hash`, `materialization`, `types`, `validation` | Wired |
| `crates/andromeda-catalog/src/snapshot.rs` | `core`, `publication`, `types`, `validation` | Wired |
| `crates/andromeda-catalog/src/wal_record.rs` | `codec`, `constants` | Wired |
| `crates/andromeda-catalog/src/procedure_feedback.rs` | `record`, `store` | Wired |
| `crates/andromeda-exec/src/local.rs` | `helpers`, `runtime`, `tests`, `types` | Wired |
| `crates/andromeda-exec/src/business.rs` | `constants`, `helpers`, `types`, `executor` | Wired |
| `crates/andromeda-exec/src/services/completion.rs` | `journal`, `mapping`, `recovery` | Wired |
| `crates/andromeda-observe/src/events.rs` | `correlation`, `decision`, `protocol_rejection`, `sequence`, `sink`, `transition` | Partially wired |
| `crates/andromeda-srpl/src/lowering.rs` | `binding`, `pipeline`, `validation` | Wired |
| `crates/andromeda-srpl/src/parser.rs` | `core`, `helpers`, `statements`, `types` | Wired |
| `crates/andromeda-storage/src/backup.rs` | `artifacts`, `helpers`, `plan`, `types` | Wired |
| `crates/andromeda-storage/src/file_wal.rs` | `format`, `header`, `recovery`, `report`, `scan`, `wal` | Wired |
| `crates/andromeda-storage/src/hadr.rs` | `fencing`, `quorum`, `types` | Wired |
| `crates/andromeda-quic/src/lib.rs` | `frame`, `rpc`, `session`, `stream` wrappers moved out of root | Wired |

## Prepared Splits Not Yet Wired

| File set | Reason | Status |
|---|---|---|
| None | Current prepared splits are wired or explicitly tracked as legacy scratch files | Clear |

## Duplicate/Facade Audit

| Area | Finding | Status |
|---|---|---|
| `andromeda-storage` layout pairs | Root files own implementation; `layout/*` files are pure facades | No merge needed |
| `andromeda-storage` WAL segment pair | Root `wal_segment.rs` owns types; `write_ahead_log/segment.rs` re-exports | No merge needed |
| `andromeda-storage` placement/wal/recovery facades | Aggregators intentionally own shared private helpers or constants | No merge needed |

## Residual Work

| Item | Priority | Notes |
|---|---|---|
| Continue `andromeda-observe/src/events.rs` split | P0 | Next extract protocol/storage/runtime payload families, then envelope validation |
| Split `crates/andromeda-catalog/src/wal_record/codec.rs` | P1 | 885 lines; codec helpers can move into reader/writer submodules |
