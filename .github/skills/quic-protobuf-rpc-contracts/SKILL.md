---
name: quic-protobuf-rpc-contracts
description: Applies QUIC plus custom Protobuf RPC contract rules when prompts mention QUIC, RPC, frames, Protobuf, admission, transport, or protocol boundaries.
license: MIT
---

# quic-protobuf-rpc-contracts

## When to use
- The user mentions QUIC, RPC, Protobuf, RPC frame, transport, mTLS, admission, streaming, or client/server protocol.
- A change touches andromeda-quic, rpc, proto, proto-wire, quic-runtime, or result stream transport.
- A review asks whether a protocol change is allowed.

## Purpose
Keep Andromeda's network surface on QUIC with a custom Protobuf RPC contract and typed admission path. The skill protects the protocol boundary from accidental gRPC adoption, JSON-native shortcuts, or security admission bypasses.

## Process
1. Read QUIC/RPC/security architecture, no-gRPC ADR, and RPC frame spec.
2. Separate protocol contract crates from runtime transport crates; do not mix Quinn runtime details into wire contracts.
3. Ensure all application calls bind to typed Procedure contracts after security admission.
4. Check frame ordering, versioning, limits, authentication context, and error mapping.
5. Add codec, interop, admission, and runtime-boundary tests without introducing gRPC servers or JSON-native APIs.

## Expected output
- A protocol-boundary summary covering QUIC, Protobuf, frames, admission, and ResultStream impact.
- Compatibility and security checks for the RPC change.
- Required crate-owned tests or fixtures.

## Reference docs
- `docs/architecture/QUIC_RPC_SECURITY_ARCHITECTURE.md`
- `docs/adr/ADR-0007-QUIC_RPC_BOUNDARY_NO_GRPC.md`
- `docs/specifications/SPEC_RPC_FRAME_V0.md`

## Guardrails
- No gRPC introduction.
- No native JSON protocol.
- No transport path that bypasses security admission or ProcedureContract binding.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
