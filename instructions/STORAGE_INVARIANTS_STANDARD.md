# Storage Invariants Standard

## Requirements

- ColdStore is immutable.
- No update-in-place in cold segments.
- No physical page split after cold publication.
- Append-only writes with validation before manifest publication.
- At least one valid cold snapshot must exist.
- WAL durability precedes visible commit.
- Page and segment formats must include checksums or hashes.

## V1 Candidate Storage Format (DEC-032)

- **Page Size:** 16 KiB or 32 KiB (to be locked after benchmarking).
- **Endianness:** Big-endian for keys (order-preserving); mixed LE/BE for headers.
- **Header:** Every page MUST start with a `PageHeader` containing `LSN`, `Checksum`, and `MagicNumber`.
- **Trailer:** Every page MUST end with a `PageTrailer` repeating the `Checksum`.
