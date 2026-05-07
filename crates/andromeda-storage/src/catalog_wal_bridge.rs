//! Bridge for encoding and decoding catalog WAL records at the storage boundary.
//!
//! This module owns the durable byte representation used by storage WAL replay.
//! `wal_record_catalog` owns the semantic record types and invariants.
//!
//! The public API is intentionally kept as a small facade. Private modules split
//! deterministic codec, record parsing, replay planning, transaction filtering,
//! format evidence, and error mapping without changing the exported bridge
//! surface.
//!
//! ## Encoding Format
//!
//! Records are encoded as a deterministic binary format using a simple
//! length-prefixed encoding scheme:
//!
//! ```text
//! [version:u16][payload_len:u32][checksum:32 bytes][payload:N bytes]
//! ```
//!
//! The checksum is computed over the payload only (not the version/length
//! prefix). This ensures round-trip encode/decode symmetry.

mod codec;
mod error;
mod format_evidence;
mod record_parsing;
mod replay_planner;
mod transaction_filter;

#[cfg(test)]
mod tests;

pub use codec::{decode_catalog_record, encode_catalog_record};
pub use replay_planner::{
    CatalogWalDurablePublication, CatalogWalPublicationRecord, CatalogWalPublicationReplayReport,
    replay_catalog_publications_from_wal,
};
