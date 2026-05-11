---
applyTo: "crates/andromeda-srpl*/**/*.rs,crates/andromeda-srpl*/**/*.toml"
---

# SRPL instructions

- Preserve SRPL contract hash canonicalization semantics from ADR-0011.
- Maintain `ProcedureContract`, `CatalogVersion`, and `ContractHash` invariants across parsing, binding, cataloging, and execution handoff.
- Do not add alternate ad hoc SQL or JSON application surfaces.
- Keep Procedure contracts typed, versioned, cataloged, and deterministic.
- Ground changes in `docs/specifications/SPEC_PROCEDURE_CONTRACT_V0.md`.
