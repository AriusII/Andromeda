---
name: andromeda-doctrine-invariants
description: Enforces Andromeda C5 invariants and procedure-only doctrine when prompts mention doctrine, C5, invariants, commit, WAL, GPU path, or dynamic SQL.
license: MIT
---

# andromeda-doctrine-invariants

## When to use
- The user says doctrine, C5, invariant, criticality, procedure-only, dynamic SQL, WAL before commit, GPU outside commit path, or mission-critical.
- A change touches admission, execution, catalog binding, transaction visibility, WAL, recovery, authorization, or accelerated execution.
- A review must decide whether an implementation violates Andromeda architecture rather than ordinary Rust style.

## Purpose
Keep every design, code change, review, and roadmap step aligned with Andromeda's non-negotiable doctrine before implementation convenience. The skill forces the agent to check mission-critical truth boundaries, procedure-only admission, durability before visibility, and evidence-bearing decisions instead of treating those items as optional style preferences.

## Process
1. Read the doctrine and criticality model first; summarize the exact invariant being protected before touching code.
2. Trace the requested path through cataloged Procedure contracts and reject any ad hoc SQL, bypass, hidden JSON protocol, or unversioned application surface.
3. Check C5 truth boundaries: no visible commit before durable WAL, no RAM-as-truth, no recovery gap, and no security-critical shortcut.
4. Classify any GPU, analytics, learned, or advisory component as outside commit, recovery, MVCC visibility, and authorization; demand scalar/CPU fallback where relevant.
5. Tie each conclusion to a reference doc and name the crate or test owner that should prove the invariant.

## Expected output
- A concise invariant checklist with pass/fail/unknown for each affected boundary.
- Specific violations, if any, with the doc path that makes them violations.
- Required validation such as WAL/recovery tests, topology tests, or contract-digest checks before claiming compliance.

## Reference docs
- `docs/project/ANDROMEDA_DOCTRINE.md`
- `docs/project/CRITICALITY_MODEL.md`
- `docs/adr/ADR-0017-NO_DYNAMIC_SQL_APPLICATION_SURFACE.md`

## Guardrails
- Do not weaken doctrine to unblock implementation speed.
- Do not introduce dynamic SQL as an application surface, gRPC, native JSON protocol, or RAM-only truth.
- Treat doctrine conflicts as blockers, not warnings.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
