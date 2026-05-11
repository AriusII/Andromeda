---
name: binary-codec-format
description: Enforces explicit little-endian binary codecs for persisted and wire formats when prompts mention codec, binary format, page, WAL, RPC frame, or serialization.
license: MIT
---

# binary-codec-format

## When to use
- The prompt mentions codec, binary format, serialization, little-endian, page format, WAL record, RPC frame, or wire compatibility.
- A change adds fields to a persisted or wire structure.
- A review sees bytemuck, transmute, repr(C), bincode, serde_json, or direct struct writes near disk/network boundaries.

## Purpose
Ensure all persistent and network formats are explicit, versioned, endian-stable, validated, and independently testable. The skill prevents serde convenience, native layout assumptions, or undocumented field ordering from becoming Andromeda's durable truth.

## Process
1. Read the canonical binary format ADR and each affected format spec before editing.
2. List magic/version/length/checksum/hash-chain/endian fields and reject implicit layout.
3. Implement explicit encode/decode with bounds checks, unknown-version behavior, and corruption errors.
4. Add golden fixture, roundtrip, truncation, endian, and invalid-field tests.
5. Document whether the change is backward compatible, forward compatible, or requires a version bump.

## Expected output
- A format-impact note naming fields, versions, and compatibility.
- Codec validation tests and golden fixture expectations.
- A clear rejection of native layout or JSON substitutions where relevant.

## Reference docs
- `docs/adr/ADR-0004-CANONICAL_BINARY_FORMAT.md`
- `docs/specifications/SPEC_PAGE_FORMAT_V0.md`
- `docs/specifications/SPEC_WAL_RECORD_V0.md`
- `docs/specifications/SPEC_RPC_FRAME_V0.md`

## Guardrails
- Never persist or transmit native Rust struct layout.
- No native JSON protocol replacement for binary RPC frames.
- No unchecked decode paths for untrusted bytes.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
