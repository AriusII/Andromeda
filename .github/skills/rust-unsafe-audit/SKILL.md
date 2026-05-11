---
name: rust-unsafe-audit
description: Audit unsafe Rust FFI SIMD mmap and raw pointer code when unsafe safety invariants or memory keywords appear.
license: MIT
---

# rust-unsafe-audit

## When to use
Use for `unsafe`, FFI, SIMD, mmap, raw pointers, aliasing, initialization, alignment, concurrency memory model, or reviewer requests about safety evidence. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill rust-unsafe-audit` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Ensure every unsafe block is private, justified, tested, and outside critical correctness paths unless explicitly designed and documented. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Find unsafe code with code search and classify by crate, invariant, caller preconditions, and failure impact.
2. Require local `// SAFETY:` explanations tied to actual invariants, not generic claims.
3. Add tests, Miri checks, fuzz/property coverage, or loom models where appropriate; run them only if already configured or available.
4. Confirm GPU/SIMD acceleration does not enter commit, rollback, WAL, recovery, MVCC visibility, or authorization paths.
5. Use read-only `/agent` auditors for independent unsafe clusters, then consolidate before proposing edits.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/adr/ADR-0003-UNSAFE_RUST_POLICY.md`
- `docs/adr/ADR-0009-GPU_OUTSIDE_COMMIT_PATH.md`
- `docs/testing/FUZZING_PLAN.md`
- `docs/testing/PROPERTY_TEST_PLAN.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not make unsafe public API.
- Do not suppress Miri/UB findings.
- Do not use performance as a reason to weaken durability or security invariants.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
