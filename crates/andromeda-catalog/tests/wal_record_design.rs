//! Integration tests for Catalog WAL record design and durability semantics.
//!
//! These tests validate:
//! 1. CatalogWalRecord field preservation and internal validation.
//! 2. Batch WAL correlation with DefinitionBatchId and LSN.
//! 3. Recovery semantics for incomplete and committed batches.
//! 4. No silent drops or incomplete recovery scenarios.

#[path = "wal_record_design/correlation.rs"]
mod correlation;
#[path = "wal_record_design/fixtures.rs"]
mod fixtures;
#[path = "wal_record_design/recovery.rs"]
mod recovery;
#[path = "wal_record_design/roundtrip.rs"]
mod roundtrip;
#[path = "wal_record_design/validation.rs"]
mod validation;
