---
name: security-iam-audit
description: "Use for mTLS, CertificateIdentity, UserPrincipal, permissions, policies, surfaces and audit."
category: andromeda-security
---

# security-iam-audit

## When to use
Security or access control is involved.

## Purpose
Use for mTLS, CertificateIdentity, UserPrincipal, permissions, policies, surfaces and audit.

## Process
- Separate certificate identity from logical user.
- Enforce surface scope and deny/allow policies.
- Emit SecurityAuditTrace for every decision.
- Keep Application, Admin and HA/DR operations separated.

## Expected output
- Security/IAM review or implementation plan.

## Guardrails
- No certificate permission bypass.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
