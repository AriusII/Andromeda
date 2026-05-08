# Crash Runner Tool

Tool for orchestrating crash injection and recovery simulation.

## Purpose

- Inject crashes at specific points
- Simulate node failures and partitions
- Replay recovery scenarios
- Validate recovery idempotency

## Usage

```bash
cargo run --bin crash-runner -- --scenario <name> [--replicas n] [--duration s]
```

## Non-Goals

- NOT part of C5 engine internals
- NO dependency on database runtime
- Pure orchestration tool for test scenarios
