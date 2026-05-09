# Andromeda Status

Last refreshed: 2026-05-09.

## Documentation Status

`/docs` is the canonical documentation entrypoint. Root navigation now points
to `/docs`, and active documentation links should use this tree.

## Workspace Status

| Item | Current status |
| --- | --- |
| Workspace crates | 88 active crates. |
| Documentation entrypoint | `/docs`. |
| Root README | Points to `/docs` and this status file. |
| Global link migration | Complete for active legacy-doc and former Loom-root links. |
| Release readiness | Not approved by documentation status alone. Use validation evidence and release gates. |

## Implementation Snapshot

- The local V0 vertical path is available through the CLI commands documented
  in the root README.
- The V0 path is still a prototype, not a production runtime, complete network
  server, or complete durable storage engine.
- Current architecture and domain contracts live under
  [architecture](architecture/README.md) and [specs](specs/README.md).
- Current implementation summaries live under
  [implementation](implementation/roadmap.md).

## Migration Notes

- The legacy documentation tree has been removed from active navigation.
- Documentation-only changes should use targeted link, status, and terminology
  checks before broader Rust validation.
