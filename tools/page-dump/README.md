# Page Dump Tool

Diagnostic tool for inspecting storage page files.

## Purpose

- Display storage page contents in human-readable format
- Inspect page headers, free space tracking
- Validate page checksums and alignment
- Extract row/index information from pages

## Usage

```bash
cargo run --bin page-dump -- <page_file> [--page <n>] [--validate] [--rows]
```

## Non-Goals

- NOT part of C5 engine internals
- NO dependency on database runtime
- Pure inspection and diagnostics only
