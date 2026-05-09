---
name: rust-unsafe-audit
description: "Use to audit unsafe Rust, FFI, SIMD, mmap, direct I/O, and raw pointer code."
category: rust-safety
---

# rust-unsafe-audit

## When to use
Unsafe code exists or is proposed.

## Purpose
Use to audit unsafe Rust, FFI, SIMD, mmap, direct I/O, and raw pointer code.

## Process
- Identify every unsafe block and its invariant.
- Require local `SAFETY:` comments and `# Safety` docs for unsafe functions.
- Check aliasing, lifetimes, alignment, initialization, target feature guards.
- Add Miri/fuzz/sanitizer/property tests when applicable.

## Expected output
- Unsafe inventory, risk, required tests, remediation.

## Guardrails
- No unsafe for convenience. No persisted struct transmute.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
