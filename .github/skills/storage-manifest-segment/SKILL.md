---
name: storage-manifest-segment
description: Guides storage segments, manifests, snapshots, root pointers, HotStore, ColdStore, and SegmentIndex work when prompts mention page, segment, manifest, or snapshot.
license: MIT
---

# storage-manifest-segment

## When to use
- The prompt mentions storage, page, segment, manifest, snapshot, root pointer, HotStore, ColdStore, SegmentIndex, or compaction.
- A change touches page codecs, disk page store, manifest, storage placement, or recovery handoff.
- A review concerns durable format compatibility or manifest switching.

## Purpose
Preserve explicit durable storage layout and root-pointer semantics across page, segment, manifest, and snapshot changes. The skill keeps HotStore/ColdStore integration, SegmentIndex lookup, and manifest publication compatible with WAL and recovery boundaries.

## Process
1. Read storage architecture, manifest, segment index, page format, and page/segment default ADRs.
2. Identify the durable object being changed and whether it affects existing bytes, manifest references, or snapshot roots.
3. Keep persisted formats codec-defined and little-endian; never use Rust struct memory layout.
4. Check WAL/recovery ordering around segment creation, manifest updates, and root-pointer switch.
5. Add roundtrip, corruption, compatibility, and recovery tests for every durable format change.

## Expected output
- A storage-object impact summary for pages, segments, manifests, and snapshots.
- Compatibility and migration expectations for existing data.
- Validation commands and corruption/recovery test cases.

## Reference docs
- `docs/architecture/STORAGE_ARCHITECTURE.md`
- `docs/specifications/SPEC_DATABASE_MANIFEST_V0.md`
- `docs/specifications/SPEC_SEGMENT_INDEX_V0.md`
- `docs/specifications/SPEC_PAGE_FORMAT_V0.md`
- `docs/adr/ADR-0006-PAGE_AND_SEGMENT_DEFAULTS.md`

## Guardrails
- No native struct layout as disk format.
- No manifest/root switch outside durability rules.
- No segment index ambiguity after recovery.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
