# andromeda-iam

## Purpose

`andromeda-iam` owns the minimal IAM runtime boundary for pre-transaction security admission.

## Scope

- Own stateless admission evaluation over authenticated principal permission evidence.
- Fail closed when principal binding, policy evidence, surface boundaries, or permission grants are missing or mismatched.
- Return a typed pre-transaction admission receipt only for admitted requests.
- Attach `andromeda-audit` security admission event vocabulary to every decision.
- Keep runtime-free permission, surface, admission, and policy-evidence vocabulary imported from `andromeda-security-contract`.

## Non-goals

- Do not own certificate parsing, mTLS extraction, QUIC/TLS runtime behavior, mutable registry persistence, audit sinks, transactions, WAL, storage, or recovery.
- Do not introduce application-facing ad hoc SQL or bypass typed Procedure contracts.
- Do not make audit evidence or RAM state durable truth.

## Prerequisites

- The caller has already authenticated transport identity and selected a security surface.
- The requested permission is a typed `andromeda-security-contract::Permission`.
- Security policy evidence is available before transaction creation.
- Legacy `LocalPrincipalResolver` and `PermissionEvaluatorImpl` consume principal identities from `andromeda-principal`; `andromeda-core` remains only a compatibility facade for external callers.

## Procedure

1. Build a `PreTransactionAdmissionRequest` with surface, class, permission, principal grants, and policy evidence.
2. Call `IamAdmissionRuntime::evaluate`.
3. Continue to transaction creation only when `PreTransactionAdmissionDecision::receipt` is present.
4. Emit or persist audit evidence outside this crate using the attached audit event.

## Validation

- Inspect `src/lib.rs` for pre-transaction admission-only behavior.
- When validating by command, use `cargo check -p andromeda-iam --all-targets`.

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| A denial still returns a receipt. | Treat this as a fail-closed regression in `IamAdmissionRuntime::evaluate`. |
| Application traffic reaches an administration or HA/DR permission. | Check `SecuritySurface`, `SurfaceClass`, and `Permission` boundaries before principal grants. |
| Policy evidence is absent but admission succeeds. | Reject the request before creating a receipt. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/andromeda-security-contract`
- `crates/andromeda-audit`
