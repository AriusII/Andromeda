---
applyTo: "crates/andromeda-storage/**/*.rs,crates/andromeda-wal/**/*.rs,crates/andromeda-recovery/**/*.rs,crates/andromeda-page/**/*.rs,crates/andromeda-manifest/**/*.rs,crates/andromeda-segment/**/*.rs,crates/andromeda-buffer-pool/**/*.rs"
---

# Storage, WAL, and recovery instructions

- Enforce WAL-before-visible-commit: no commit may become visible until the required WAL durability point is satisfied.
- Preserve durability fences, recovery floors, and crash-recovery evidence.
- Never use native Rust struct layout as disk format; use explicit binary codecs with validation tests.
- Keep recovery behavior deterministic and compatible with manifest, segment, page, and buffer-pool ownership boundaries.
- Reference ADR-0004, ADR-0005, ADR-0006, and `docs/specifications/SPEC_WAL_RECORD_V0.md`.
