# Security Test Index

## Purpose

This directory is a documentation index for security validation. It does not own an executable Rust harness.

Executable security tests remain with the crates that own certificate identity, permission evaluation, policy versions, audit evidence, RPC admission, and executor authorization behavior.

## Scope

Use this index for roadmap entries that refer to `tests/security`.

Entries should name the owning crate, protected surface, principal or policy model, audit evidence, expected denial or admission behavior, and validation command.

## Test destination

| Scenario type | Destination |
| --- | --- |
| Certificate identity, principal binding, policy, or permission evaluation | The owning IAM, security, core, or contract crate's tests. |
| RPC admission, zero-RTT policy, transport identity, or session security | The owning RPC or QUIC crate's tests. |
| Procedure authorization, denial, audit completion, or executor admission | The owning executor, admission, audit, or observability crate's tests. |
| Admin or HA/DR protection checks | The owning admin, HA/DR, backup, restore, or security crate's tests; never the Application Surface. |
| Fuzz-discovered parser, codec, or policy input defect | A deterministic crate-local security regression; keep fuzz target and corpus metadata in `fuzz/` and index them through `tests/fuzzing/`. |
| Root roadmap security coverage | This README, as an index entry that points to the owning crate command. |

## Non-goals

- Do not expose Administration or HA/DR capabilities through the Application Surface.
- Do not bypass typed Procedure contracts or permission checks.
- Do not accept unaudited security-critical behavior.
- Do not create root-level security harnesses without an explicit future work order.
- Do not duplicate crate-local security tests, fuzz targets, seed corpora, or generated fixtures in this directory.

## Prerequisites

- Review `tests/README.md`.
- Identify the IAM, contract, RPC, executor, or audit crate that owns the behavior.

## Procedure

1. Map the security scenario to the owning crate.
2. Keep executable IAM, permission, audit, transport, or executor tests in that crate.
3. Convert fuzz-discovered input handling failures into deterministic crate-local security regressions before citing them here.
4. Record the principal, policy, denial or admission rule, audit evidence, and validation command in this index when the scenario is ready.

## Validation

For this documentation index, run:

```powershell
rg -n "Purpose|Scope|Validation" tests/security
```

Runtime validation belongs to the owning security, IAM, contract, RPC, executor, or audit crate test command.

## Troubleshooting

If a scenario needs both transport identity and Procedure permission evidence, record both owners and keep this directory as the index only.

## References

- `tests/README.md`
- `docs/codex/mission-critical-change-policy.md`
- `docs/codex/rust-critical-quality-gates.md`
- `documentations/testing/step-11-validation-matrix.md`
