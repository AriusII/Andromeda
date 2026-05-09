# andromeda-optimizer

## Purpose

`andromeda-optimizer` owns bounded optimizer policy and runtime-free plan-input DecisionTrace production.

The crate currently validates optimizer input ownership: complete plan-cache identity, published statistics compatibility, policy-version compatibility, and disableable adaptive fallback. SRPL compilation and executable plan construction remain in their current owners.

## Scope

This crate is expected to own:

- Bounded candidate enumeration and rejected-candidate evidence.
- Cost model terms that are observable, versioned, explainable, and disableable.
- Plan class rules and fallback policy.
- Consumption of `StatsVersion`, catalog version, contract hash, policy version, and ScenarioEvidence after validation.
- DecisionTrace records explaining the chosen plan, ignored evidence, stale inputs, and fallback reasons.

## Non-goals

- Do not let benchmark output, ScenarioEvidence, learned components, or GPU output select a plan alone.
- Do not bypass typed Procedure contracts or cataloged procedure bindings.
- Do not treat optimizer estimates as storage, catalog, WAL, or recovery truth.
- Do not put optimizer or GPU work in C5 commit, WAL, rollback, recovery, MVCC short-visibility, catalog publication, or security-critical paths.
- Do not introduce ad hoc SQL or dynamic shape-shifting returns.

## Prerequisites

- Keep SRPL compilation and executable optimizer behavior in `andromeda-srpl` and catalog-facing crates until a registered extraction work order moves it.
- Require complete plan identity: procedure contract, catalog version, stats version, policy version, and plan class.
- Require a deterministic fallback when advisory evidence is missing, stale, disabled, or unsafe.

## Procedure

1. Define plan candidate and cost identities before moving optimizer logic.
2. Keep plan choice bounded by explicit limits and stop rules.
3. Validate statistics and ScenarioEvidence before considering them.
4. Emit DecisionTrace for accepted, rejected, stale, and ignored inputs.
5. Preserve compatibility with current SRPL and catalog-facing paths during extraction.

## Validation

Future behavior changes should use:

```powershell
cargo test -p andromeda-optimizer
cargo test -p andromeda-optimizer --test srpl_optimizer_pipeline_contract -- --nocapture
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

Run package checks and focused tests when changing these contracts.

## Troubleshooting

- If a plan cannot be explained, add DecisionTrace fields before accepting the path.
- If plan choice changes because benchmark evidence exists, verify that bounded optimizer logic still made the final decision.
- If a GPU or analytics crate enters a C5 dependency path, reject the dependency.

## References

- [Workspace crate rules](../README.md)
- [Current SRPL compatibility surface](../andromeda-srpl/README.md)
- [Current catalog owner](../andromeda-catalog/README.md)
