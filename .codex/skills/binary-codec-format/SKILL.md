---
name: binary-codec-format
description: "Use for explicit binary codecs, persisted files, WAL, pages, manifests, frames and contracts."
category: andromeda-storage
---

# binary-codec-format

## When to use
A binary format is designed or edited.

## Purpose
Use for explicit binary codecs, persisted files, WAL, pages, manifests, frames and contracts.

## Process
- Use little-endian canonical fields.
- Use magic, version, feature flags, lengths, CRC/hash and bounds checks.
- Never serialize Rust-native structs.
- Hash canonical encoded forms.

## Expected output
- Codec design or audit.

## Guardrails
- No `repr(Rust)` persisted layouts.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
