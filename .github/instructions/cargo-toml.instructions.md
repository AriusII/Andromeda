---
applyTo: "**/Cargo.toml"
---

# Cargo.toml instructions

- Preserve workspace lint policy: `unsafe_op_in_unsafe_fn`, `unreachable_pub`, and `missing_docs` stay warnings; Clippy correctness and suspicious groups stay denied.
- Do not add target-specific `target-cpu` overrides or portability-breaking optimization flags.
- Keep `rust-version` pinned to the repository MSRV policy of Rust 1.85+ per ADR-0001 unless the task explicitly updates the policy.
- Follow supply-chain controls from ADR-0010: prefer narrow dependencies, avoid duplicate ecosystems, and justify new runtime dependencies.
- Keep crate ownership explicit; do not create generic utility buckets.
