# Schema Gen Tool

Tool for generating schema-related code from specifications.

## Purpose

- Generate Protobuf code from specifications
- Generate codec implementations
- Generate type definitions from schemas
- Validate schema consistency

## Usage

```bash
cargo run --bin schema-gen -- --schema <spec> --output <dir> [--language rust|go]
```

## Non-Goals

- NOT part of C5 engine internals
- NO dependency on database runtime
- Pure code generation only
