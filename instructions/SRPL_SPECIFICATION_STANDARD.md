# SRPL Specification Standard

## Scope

Use for SRPL syntax, semantics, compiler, and procedure contracts.

## Requirements

- SRPL remains procedure-first and transaction-first.
- No ad hoc SQL surface.
- No unbounded loops in the core language.
- Explicit cardinality is preferred over inferred hope.
- Absence must be typed and explicit.
- Result ordering is not contractual unless declared.
- Every procedure must bind to a ContractHash, CatalogVersion, StatsVersion, and PolicyVersion.

## Output requirements

When proposing syntax, include grammar sketch, semantic constraints, IR lowering, validation rules, and examples.
