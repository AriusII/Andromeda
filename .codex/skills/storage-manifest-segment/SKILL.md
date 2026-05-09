---
name: storage-manifest-segment
description: "Use for storage segments, manifests, snapshots, root pointers, HotStore/ColdStore and SegmentIndex."
category: andromeda-storage
---

# storage-manifest-segment

## When to use
Storage layout or physical files are involved.

## Purpose
Use for storage segments, manifests, snapshots, root pointers, HotStore/ColdStore and SegmentIndex.

## Process
- Use segmented append-only formats, immutable ColdStore and reconstructible HotStore.
- Use signed/verified manifests and SegmentIndex for startup.
- No update-in-place cold snapshot.
- No compression globally across blocks.

## Expected output
- Storage format plan or audit.

## Guardrails
- No monolithic file, no file-per-page design.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
