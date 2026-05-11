---
name: rust-performance-optimization
description: Make measured Rust performance improvements when benchmark latency throughput criterion divan or optimization keywords appear.
license: MIT
---

# rust-performance-optimization

## When to use
Use for latency, throughput, allocation, benchmark, Criterion, Divan, regression, hot path, or performance optimization requests. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill rust-performance-optimization` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Improve performance with a measured baseline, bounded hypothesis, and validation that critical database invariants remain intact. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Define the metric, workload, and owner crate before editing; use existing benches under owning crates rather than root-owned runtime tests.
2. Run available Criterion/Divan or cargo bench targets; if absent, add minimal owner-local benchmarks only when requested or necessary.
3. Change one hypothesis at a time and compare before/after results with command lines and environment caveats.
4. Run correctness gates after performance changes, especially WAL/recovery/security-adjacent paths.
5. Use `/agent` read-only profilers or code scouts for independent hotspots, then consolidate into one implementation plan.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/architecture/HARDWARE_ARCHITECTURE.md`
- `docs/testing/CI_GATES.md`
- `docs/testing/RELEASE_GATES.md`
- `docs/adr/ADR-0009-GPU_OUTSIDE_COMMIT_PATH.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not claim improvement without measurements.
- Do not place benches in ownership-inappropriate root `tests/`.
- Do not move GPU/accelerated code into commit, recovery, or security-critical paths.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
