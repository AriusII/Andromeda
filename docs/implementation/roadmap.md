# Implementation Roadmap

Last refreshed: 2026-05-09.

This page is a concise implementation roadmap for the `/docs` surface. It is
not release approval and does not replace validation evidence.

## Current Position

- The workspace has 88 active crates.
- `/docs` is the canonical documentation entrypoint.
- The local V0 vertical path exists for CLI smoke and recovery inspection.
- The V0 path remains a prototype until durable storage, network runtime,
  recovery, security, and release-gate evidence are complete.

## Near-Term Priorities

1. Keep new documentation links anchored in `/docs`.
2. Preserve direct owner crate boundaries and avoid reintroducing removed
   compatibility scaffolds.
3. Continue caller migration toward owner crates where broad facades still hide
   ownership boundaries.
4. Complete validation for WAL durability, recovery replay, protocol framing,
   security admission, audit evidence, backup, restore, and HA/DR operations.
5. Refresh status docs from command output after validation completes.
6. Keep testing scripts and runbooks aligned with `/docs` and
   `tools/loom-models`.

## Deferred Until Evidence Exists

- Production readiness claims.
- Complete server/runtime claims.
- Complete durable storage claims.
- Broad readiness claims without retained evidence.
