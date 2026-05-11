---
name: protobuf-no-grpc-no-json
description: Enforces Protobuf without gRPC and no JSON native protocol when prompts mention gRPC, JSON API, REST, Protobuf transport, or protocol alternatives.
license: MIT
---

# protobuf-no-grpc-no-json

## When to use
- The prompt suggests gRPC, tonic, REST, HTTP JSON, JSON protocol, OpenAPI, or replacing the RPC surface.
- A dependency or example adds grpc, tonic server, prost service bindings, serde_json RPC payloads, or HTTP application execution.
- A review needs to distinguish debug/export JSON from native protocol JSON.

## Purpose
Provide a fast policy check for protocol-surface proposals. The skill allows Protobuf messages within Andromeda's custom QUIC RPC but blocks gRPC, REST/JSON as the native application protocol, and any implementation that sidesteps typed procedure admission.

## Process
1. Read the no-gRPC ADR and QUIC/RPC/security architecture.
2. Classify the proposed use: internal debug artifact, docs/example, import/export, or native application protocol.
3. Reject gRPC service runtime and JSON-native application execution; route allowed calls through QUIC custom Protobuf RPC.
4. Confirm ProcedureContract binding and security admission still happen before execution.
5. Suggest compliant alternatives: explicit Protobuf messages, RPC frame codecs, typed ResultStream, and crate-owned protocol tests.

## Expected output
- An allow/reject decision with the exact protocol reason.
- A compliant replacement design if the proposal is rejected.
- Tests or dependency removals needed to enforce the boundary.

## Reference docs
- `docs/adr/ADR-0007-QUIC_RPC_BOUNDARY_NO_GRPC.md`
- `docs/architecture/QUIC_RPC_SECURITY_ARCHITECTURE.md`

## Guardrails
- Do not add gRPC runtime, service definitions as the application boundary, or tonic server behavior.
- Do not accept JSON as the native protocol.
- Do not confuse diagnostic JSON with protocol truth.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
