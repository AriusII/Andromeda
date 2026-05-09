---
name: protobuf-no-grpc-no-json
description: "Use to enforce Protobuf without gRPC and no JSON native protocol."
category: andromeda-protocol
---

# protobuf-no-grpc-no-json

## When to use
Any protocol or serialization design mentions JSON/gRPC.

## Purpose
Use to enforce Protobuf without gRPC and no JSON native protocol.

## Process
- Check native protocol is QUIC + custom Protobuf.
- Permit JSON only for optional tooling/docs if explicitly out-of-band and not native surface.
- Reject gRPC semantics, gRPC service stubs, HTTP/JSON fallback, and JSON schema as contract source.

## Expected output
- Protocol compliance finding.

## Guardrails
- Do not introduce JSON for convenience.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
