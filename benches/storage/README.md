# Storage Benchmark Suite

Benchmarks for storage layer performance characteristics.

## Scenarios

- Page read/write throughput
- B-tree lookup latency
- Full table scan throughput
- Index build time
- Cold-store segment migration

## Hardware Profiles

Specify when adding benchmarks:
- CPU model and core count
- Storage device (SSD/NVMe model, interface)
- Memory/buffer pool size
- Kernel/OS

## Non-Goals

Benchmarks are advisory only. Must not replace:
- Storage correctness tests
- Crash/recovery validation
- MVCC visibility gates
