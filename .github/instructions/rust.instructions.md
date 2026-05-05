---
applyTo: "**/*.rs"
---
# Rust Instructions for Andromeda
- Maintain deterministic behavior.
- Keep unsafe blocks minimal, documented, and tested.
- Prefer explicit error types over stringly-typed errors.
- Do not panic in runtime paths that may be reached by a Procedure invocation.
- Preserve WAL-before-visible-commit invariants.
- Use strongly typed IDs, versions, hashes, and LSN values.
