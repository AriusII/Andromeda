# andromeda-scenario-evidence

## Purpose

`andromeda-scenario-evidence` owns advisory benchmark evidence, benchmark history records, ScenarioEvidence boundary records, validity windows, confidence policy, target bindings, and advisory evidence lifecycle.

`andromeda-bench` reexports this crate for compatibility. Evidence from this crate remains advisory only.

## Scope

This crate owns:

- Benchmark evidence, benchmark history records, confidence bounds, and validity windows.
- Target bindings for procedure, contract hash, catalog version, stats version, policy version, and plan class.
- Advisory use statuses such as accepted, stale, expired, target mismatch, disabled, and ignored.
- Conversion boundaries from benchmark runs into expirable evidence records.
- DecisionTrace inputs that explain evidence use or rejection.

## Non-goals

- Do not make ScenarioEvidence authoritative for optimizer, statistics, catalog, storage, WAL, recovery, or security behavior.
- Do not let benchmark output create trusted evidence without version, target, confidence, and validity checks.
- Do not bypass crash/recovery, security, typed Procedure, WAL, or protocol validation.
- Do not put ScenarioEvidence production, analytics, or GPU work in C5 commit, WAL, rollback, recovery, MVCC short-visibility, catalog publication, or security-critical paths.
- Do not persist records through native Rust struct layout.

## Prerequisites

- Keep catalog-side authoritative ScenarioEvidence publication outside this crate.
- Require every record to be expirable, bounded, version-bound, and disableable.
- Require DecisionTrace when evidence affects an adaptive decision.

## Procedure

1. Validate target identity before accepting evidence.
2. Validate confidence, validity window, budget, and measurement mode.
3. Mark every evidence record advisory and non-authoritative.
4. Reject stale, expired, mismatched, or disabled evidence before optimizer use.
5. Preserve current catalog and benchmark compatibility paths until callers migrate.

## Validation

Behavior changes should use:

```powershell
cargo check -p andromeda-scenario-evidence --tests
cargo check -p andromeda-bench --tests
```

## Troubleshooting

- If evidence lacks a target version, reject it.
- If evidence is expired or not yet valid, ignore it and record that status in DecisionTrace.
- If evidence appears to force a plan, move the decision back to bounded optimizer logic.

## References

- [Workspace crate rules](../README.md)
- [Current benchmark owner](../andromeda-bench/README.md)
- [Current catalog owner](../andromeda-catalog/README.md)
