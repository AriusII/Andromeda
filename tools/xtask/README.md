# Andromeda Tools: xtask

`xtask` is a Rust project orchestrator for common development tasks.

## Purpose

xtask provides a single entry point for:
- Building and testing
- Code generation (schema, codec)
- Profiling and benchmarking
- Fuzzing orchestration
- Development utilities

## Usage

```bash
# Run help
cargo xtask --help

# Common tasks (examples - TBD)
cargo xtask build
cargo xtask test --profile recovery
cargo xtask fuzz --duration 3600
cargo xtask bench --profile wal
cargo xtask format
cargo xtask lint
```

## Design

- Pure Rust implementation
- No external shell dependencies
- Cross-platform (Windows, Linux, macOS)
- Clear error messages
- Deterministic task execution

## Tasks (Placeholder)

- `build` - Compile workspace
- `test` - Run test suite
- `fuzz` - Run fuzzing targets
- `bench` - Run benchmarks
- `format` - Format code
- `lint` - Run linters
- `ci` - Full CI pipeline

## Non-Goals

xtask is NOT:
- A general build system (use Cargo for that)
- A CI/CD system (use GitHub Actions)
- A package manager
- Tied to C5 engine internals
