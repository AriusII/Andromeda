---
name: quic-protobuf-rpc-contracts
description: "Use for QUIC + Protobuf custom RPC design and implementation."
category: andromeda-protocol
---

# quic-protobuf-rpc-contracts

## When to use
Protocol, transport, frames or RPC surface are involved.

## Purpose
Use for QUIC + Protobuf custom RPC design and implementation.

## Process
- Keep QUIC as transport and Andromeda RPC as semantics.
- Use Protobuf messages/frames; no gRPC and no JSON native path.
- Validate FrameHeader, payload length and surface scope.
- Keep Application, Administration and HA/DR surfaces separated.

## Expected output
- Protocol contract review or implementation plan.

## Guardrails
- No gRPC service surface. No JSON payload contract.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
