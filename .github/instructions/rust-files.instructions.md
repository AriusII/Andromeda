---
applyTo: "**/*.rs"
---

# Rust file instructions

- Target the repository Rust baseline and keep code compatible with Rust 1.95 unless the task explicitly changes policy.
- Keep changes `cargo clippy` clean; do not add allowances to silence issues without a documented reason.
- Do not use `unwrap()` outside tests. Prefer typed errors or explicit handling.
- Do not introduce panics in library code; return typed errors instead.
- Any `unsafe` must be private, minimal, documented, and justified under `docs/adr/ADR-0003-UNSAFE_RUST_POLICY.md`.
- Do not build protocol, disk, or application-surface payloads with `format!`; use explicit binary codecs.
- Do not introduce JSON as an application execution surface; Andromeda uses typed Procedure contracts and binary/wire codecs.
