# Source cross-check

> **Status:** Traceability reference  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Record the source basis used to prepare the documentation package.
- Separate stable doctrine from research-front material.
- Preserve traceability without copying old files into the replacement package.

## Source categories

| Category | Role |
|---|---|
| Consolidated Andromeda Markdown | Internal doctrine, architecture, SRPL, transaction, storage, QUIC/RPC, security, optimizer, hardware. |
| SRPL foundation material | Strict relational procedure language, typed absence, cardinality, compilation to IR. |
| RDBMS research corpus | Relational model, ACID mechanisms, isolation, WAL/ARIES, normalization, statistics, B+Trees. |
| Secure storage material | Segmented storage, manifests, WAL, HotStore, ColdStore, SegmentIndex. |
| Rust engineering material | Rust 1.95.0 baseline, unsafe policy, workspace governance, testing, fuzzing, supply chain. |
| CPU/GPU research material | Runtime feature detection, SIMD fallback, GPU batch analytics outside commit path. |
| GitHub repository state | Current modular Rust foundation and vertical recoverable prototype direction. |

## Stable source conclusions

| Conclusion | Documentation impact |
|---|---|
| Relational foundations remain valid. | Andromeda keeps relational modeling and algebraic optimization. |
| ACID must be tied to mechanisms. | Docs describe WAL, isolation, anomalies, commit, and recovery concretely. |
| Snapshot isolation is not automatically serializable. | Isolation specs require anomaly-level explanation. |
| WAL/recovery is core truth. | Storage and transaction docs put WAL before visibility. |
| Learned components are not universal replacements. | Optimizer docs classify them as evidence or experiment. |
| B+Tree remains the default ordered access path. | Storage docs use B+Tree as baseline and learned indexes as optional. |
| GPU is useful for analytics/statistics. | GPU docs isolate it from C5 paths. |
| Rust requires explicit unsafe governance. | Rust docs require unsafe review, Miri, fuzzing, and no native layout persistence. |

## Research-front conclusions

| Topic | Documentation position |
|---|---|
| Learned cardinality estimation | Experimental until validated against robust baselines. |
| Learned indexes | Optional analytics only with B+Tree fallback. |
| Learned optimizer | Evidence or suggestion, not sole decision-maker. |
| Deterministic concurrency | Serious direction, but workload-dependent. |
| GPU acceleration | Batch-only accelerator. Not transaction participant. |

## Repository alignment

The documentation assumes a modular Rust repository centered on catalog, SRPL, protocol, QUIC, execution, transaction, storage, observability, benchmark, and CLI crates.

The roadmap prioritizes this path:

```text
Procedure
-> Catalog
-> SRPL IR
-> Admission
-> Transaction
-> WAL
-> Heap/Page
-> Recovery
-> ResultStream
```
