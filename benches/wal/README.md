# WAL Benchmark Suite

Benchmarks for Write-Ahead Log performance characteristics.

## Scenarios

- WAL record write throughput
- WAL record read/scan throughput
- Checkpoint latency
- Recovery replay throughput
- Flush synchronization overhead

## Hardware Profiles

Specify when adding benchmarks:
- CPU model and core count
- Storage device (SSD/NVMe model, interface)
- Memory capacity
- Kernel/OS

## Non-Goals

Benchmarks are advisory only. Must not replace:
- WAL durability gates
- Crash/recovery correctness tests
- Replay idempotency validation
