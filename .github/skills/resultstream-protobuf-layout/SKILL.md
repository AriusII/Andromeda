---
name: resultstream-protobuf-layout
description: Maintains ResultStream metadata-before-payload Protobuf layout when prompts mention ResultStream, streaming results, payload order, metadata, or row frames.
license: MIT
---

# resultstream-protobuf-layout

## When to use
- The prompt mentions ResultStream, result ordering, metadata-before-payload, row frame, streaming payload, client decode, or Protobuf result layout.
- A change touches andromeda-result-stream, RPC codecs, execution output, or inventory demo streaming.
- A test fails around result ordering or metadata availability.

## Purpose
Keep streamed procedure results deterministic and decodable by requiring metadata before payload. The skill protects clients, tests, and RPC framing from changes that make rows or chunks arrive before schema, status, or contract metadata.

## Process
1. Read the ResultStream spec and P02 ordering contract before changing any stream shape.
2. Identify all metadata required before payload: contract identity, schema/result shape, status context, ordering, and limits.
3. Reject any implementation that emits data chunks before required metadata is sent and accepted.
4. Check backpressure and slow-client behavior without changing the logical metadata ordering.
5. Add stream-order tests proving metadata-first behavior for success, error, empty, and interrupted streams.

## Expected output
- A ResultStream ordering statement and affected frame sequence.
- Client compatibility risks and required migration notes.
- Concrete tests for metadata-first, payload, completion, and error frames.

## Reference docs
- `docs/specifications/SPEC_RESULT_STREAM_V0.md`
- `docs/roadmap/sources/P02_RESULT_STREAM_ORDERING_CONTRACT.md`

## Guardrails
- No payload before required metadata.
- No JSON result stream substitute.
- Do not hide ordering bugs under async timing assumptions.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
