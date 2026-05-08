# WAL Dump Tool

Diagnostic tool for inspecting Write-Ahead Log (WAL) files.

## Purpose

- Display WAL record contents in human-readable format
- Validate WAL structure and checksums
- Extract record timestamps and LSN ranges
- Forensic analysis of WAL files

## Usage

```bash
cargo run --bin wal-dump -- <wal_file> [--lsn-range <start>..<end>] [--validate]
```

## Non-Goals

- NOT part of C5 engine internals
- NO dependency on database runtime
- Pure inspection and diagnostics only
