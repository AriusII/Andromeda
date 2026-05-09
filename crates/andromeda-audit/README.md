# andromeda-audit

## Purpose

`andromeda-audit` owns typed audit event vocabulary for security, IAM admission, administration, HA/DR, backup, durable audit journal replay, and review evidence.

## Scope

- Own stable audit event shapes, labels, durable replay query rows, and journal evidence.
- Preserve security admission v0 evidence as typed codes from `andromeda-security-contract`.
- Reuse shared observability identifiers without owning the observability runtime.
- Keep audit events suitable for review and forensic correlation.

## Non-goals

- Do not make audit output authorization, catalog, storage, transaction, WAL, or recovery truth.
- Do not make durable audit replay inspection depend on `andromeda-observe::EventEnvelope`.
- Do not parse certificates, evaluate IAM policy, persist registries, or perform transport work.

## Prerequisites

- Security admission vocabulary comes from `andromeda-security-contract`.
- Runtime IAM admission belongs in `andromeda-iam`.
- Event-envelope adapters remain outside this crate.

## Procedure

1. Add event families as typed Rust values with stable string labels.
2. Keep security admission event fields tied to contract vocabulary, not ad hoc strings.
3. Keep persistence and wire encoding outside this crate unless an explicit codec owner is added.
4. Reject events that need runtime authorization decisions in this crate.

## Validation

- Inspect `src/lib.rs` and event modules for vocabulary-only behavior.
- When validating by command, use `cargo check -p andromeda-audit --all-targets`.
- Use `cargo test -p andromeda-audit --test admission_audit_contract --test hadr_backup_audit_contract -- --nocapture` for admission, backup, and HA/DR audit vocabulary.

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| An audit event decides authorization. | Move the decision to `andromeda-iam` and keep only event vocabulary here. |
| A security admission event uses stringly typed permission or surface fields. | Use `andromeda-security-contract` types. |
| Audit output becomes storage or recovery truth. | Move durable truth to the WAL/storage/recovery owner and keep audit as evidence. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/andromeda-security-contract`
- `crates/andromeda-iam`
