---
name: security-iam-audit
description: Guides mTLS, CertificateIdentity, UserPrincipal, permissions, surfaces, and audit work when prompts mention security, IAM, admission, permission, or audit ledger.
license: MIT
---

# security-iam-audit

## When to use
- The user mentions security, IAM, mTLS, CertificateIdentity, UserPrincipal, permission, admission, authorization, audit, break-glass, or incident.
- A change touches andromeda-security, iam, principal, audit, quic admission, or execution dispatch.
- An incident or review concerns who could execute which procedure and what was recorded.

## Purpose
Ensure every executable surface is authenticated, authorized, policy-versioned, and audit-evidenced. The skill keeps security admission ahead of contract binding and execution while preserving durable audit records for mission-critical operations.

## Process
1. Read QUIC/RPC/security architecture, security admission spec, audit ledger spec, and incident runbook.
2. Trace identity from mTLS certificate through principal binding, permission evaluation, policy version, contract binding, and execution.
3. Ensure denial is fail-closed and audited where required without leaking sensitive data.
4. Verify audit entries are durable, ordered, and correlate request, principal, procedure, decision, and result.
5. Add authorization matrix, admission rejection, audit durability, and incident-oriented tests.

## Expected output
- A security path trace from certificate to audit ledger.
- Permission and audit evidence requirements for the change.
- Fail-closed behavior and incident/runbook impacts.

## Reference docs
- `docs/architecture/QUIC_RPC_SECURITY_ARCHITECTURE.md`
- `docs/specifications/SPEC_SECURITY_ADMISSION_V0.md`
- `docs/specifications/SPEC_AUDIT_LEDGER_V0.md`
- `docs/runbooks/RUNBOOK_SECURITY_INCIDENT.md`

## Guardrails
- No execution before security admission.
- No authorization based only on unauthenticated user text.
- No GPU or learned component in authorization-critical paths.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
