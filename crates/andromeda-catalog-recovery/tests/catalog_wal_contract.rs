//! Comprehensive contract tests for catalog WAL record encoding and recovery.
//!
//! Categories:
//! - encode/decode round-trip tests;
//! - version monotonicity and ordering tests;
//! - procedure ID validation tests;
//! - checksum and corruption tests;
//! - recovery replay tests;
//! - storage WAL publication bridge tests.

#[path = "catalog_wal_contract/support.rs"]
mod support;

#[path = "catalog_wal_contract/checkpoint.rs"]
mod checkpoint;
#[path = "catalog_wal_contract/corruption_decode.rs"]
mod corruption_decode;
#[path = "catalog_wal_contract/encode_decode.rs"]
mod encode_decode;
#[path = "catalog_wal_contract/publication_bridge.rs"]
mod publication_bridge;
#[path = "catalog_wal_contract/replay_lifecycle.rs"]
mod replay_lifecycle;
#[path = "catalog_wal_contract/replay_validation.rs"]
mod replay_validation;
