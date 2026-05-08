# Catalog Diff Tool

Diagnostic tool for comparing catalog snapshots.

## Purpose

- Compare catalog versions or snapshots
- Display schema evolution
- Identify breaking/non-breaking changes
- Validate catalog consistency

## Usage

```bash
cargo run --bin catalog-diff -- <snapshot1> <snapshot2> [--detailed]
```

## Non-Goals

- NOT part of C5 engine internals
- NO dependency on database runtime
- Pure comparison and diagnostics only
