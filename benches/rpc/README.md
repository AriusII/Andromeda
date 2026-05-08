# RPC/QUIC Benchmark Suite

Benchmarks for RPC and QUIC protocol performance characteristics.

## Scenarios

- Message round-trip latency
- Throughput (messages/sec)
- Connection establishment time
- Stream open/close overhead
- Backpressure handling efficiency

## Hardware Profiles

Specify when adding benchmarks:
- Network topology (local/remote)
- MTU and buffer sizes
- CPU and memory constraints

## Non-Goals

Benchmarks are advisory only. Must not replace:
- Protocol correctness tests
- Frame format validation
- RPC contract enforcement
- IAM and audit gates
