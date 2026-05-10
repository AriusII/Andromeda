# andromeda-security-contract

## Purpose

`andromeda-security-contract` owns Andromeda's runtime-free security vocabulary: security surfaces, surface planes, permission families, canonical permission identifiers, policy evidence shapes, and admission evidence codes.

Use this crate when code needs stable security contract labels or explicit semantic mappings. Do not treat it as an IAM runtime, policy store, role store, certificate parser, revocation database, authorization evaluator, audit sink, or transport runtime.

## Scope

This crate is responsible for:

| Area | Responsibility |
| --- | --- |
| Surfaces | Stable `Application`, `Administration`, `Cluster`, `BackupAgent`, and `MonitoringAgent` surfaces. |
| Surface planes | Explicit mapping from public planes to required certificate scopes. |
| Permissions | Canonical permission identifiers and permission families for application, definition, diagnostics, security, recovery, and cluster operations. |
| Surface rules | Runtime-free checks for whether a surface permits a permission family. |
| Policy evidence | Fixed-length security policy versions and bounded policy evidence descriptors. |
| Admission evidence | Stable SecurityAdmission v0 steps, evidence codes, outcomes, reason codes, boundaries, and contract id. |

The crate defines contract vocabulary. It does not decide whether a principal is authorized in a live deployment.

## Non-goals

- Do not introduce application-facing ad hoc SQL, generic command text, dynamic table names, or dynamic predicates.
- Do not bypass typed, cataloged Procedure contracts. Application permissions apply to typed Procedure execution and contract metadata access only.
- Do not expose Administration, recovery, security management, backup, restore, forensic startup, or cluster permissions through the Application Surface.
- Do not own mutable IAM state, role assignment, policy storage, revocation storage, certificate parsing, mTLS extraction, QUIC/TLS runtime behavior, audit ledgers, WAL, storage, or recovery.
- Do not serialize Rust native structs directly to disk or network. Persistent and network boundaries must use explicit codecs or generated protocol contracts.
- Do not use enum ordinal ordering as a security decision. Security decisions must use explicit mappings.

## Prerequisites

Before changing this crate, understand:

- The Application Surface permits only the application permission family.
- The Administration Surface covers definition, diagnostics, security, and recovery families, but remains separate from Application dispatch.
- Cluster, BackupAgent, and MonitoringAgent surfaces have their own restricted permission families.
- SecurityAdmission v0 is a pre-transaction boundary and evidence vocabulary; full runtime IAM behavior lives elsewhere.
- Permission identifiers are public contract labels and must remain stable unless a compatibility decision says otherwise.

## Procedure

1. Add new permissions by defining a canonical lowercase identifier, a `Permission` variant, a `PermissionFamily`, round-trip mapping, and tests.
2. Add new surfaces only with explicit plane binding and surface-to-permission-family rules.
3. Keep Application permissions narrow. Application traffic must not gain Administration, recovery, security management, or cluster authority.
4. Keep security vocabulary runtime-free. If a change needs mutable state, certificate parsing, policy evaluation, revocation lookup, audit persistence, or network I/O, move it to the owning runtime crate.
5. Keep admission codes stable and descriptive enough for audit and operator evidence.
6. Reject ordinal enum comparisons for authorization or surface separation. Use named variants and explicit match expressions.

## Validation

For documentation-only changes, validate this README against `AGENTS.md`, `docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md`, and `docs/adr/ADR-0007-QUIC_RPC_BOUNDARY_NO_GRPC.md`.

For code changes in this crate, prefer:

```bash
cargo fmt --all --check
cargo test -p andromeda-security-contract --tests
cargo check -p andromeda-security-contract --all-targets
```

If a change affects surface separation or permission mapping, include targeted tests that prove Application, Administration, Cluster, BackupAgent, and MonitoringAgent permissions remain distinct.

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| Application is allowed to manage security, restore, or promote cluster state. | Move that permission to the Administration, recovery, security, or cluster surface and reject it from Application. |
| A caller needs to know whether a live principal is authorized. | Use this crate's vocabulary, but perform authorization in the IAM runtime owner. |
| A new permission uses mixed case or aliases. | Replace it with one canonical lowercase identifier and round-trip tests. |
| Security logic compares enum ordinal values. | Replace ordinal logic with explicit match-based mappings. |
| A policy version is treated as database truth. | Treat it as security evidence; database truth remains the latest valid cold snapshot plus durable WAL. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md`
- `docs/adr/ADR-0007-QUIC_RPC_BOUNDARY_NO_GRPC.md`
- `crates/andromeda-security-contract/src/lib.rs`
- `crates/andromeda-security-contract/src/surface.rs`
- `crates/andromeda-security-contract/src/permission.rs`
- `crates/andromeda-security-contract/src/policy.rs`
- `crates/andromeda-security-contract/src/admission.rs`
