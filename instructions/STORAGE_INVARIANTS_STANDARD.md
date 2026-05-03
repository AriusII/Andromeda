# Storage Invariants Standard

## Requirements

- ColdStore is immutable.
- No update-in-place in cold segments.
- No physical page split after cold publication.
- Append-only writes with validation before manifest publication.
- At least one valid cold snapshot must exist.
- WAL durability precedes visible commit.
- Page and segment formats must include checksums or hashes.
