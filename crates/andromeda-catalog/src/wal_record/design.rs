//! Design record for Catalog WAL Record types and durability semantics.
//!
//! # E4: Catalog WAL Records Design
//!
//! ## Overview
//!
//! This module defines the high-level WAL record types for catalog mutations and
//! documents the durability semantics that ensure recovery consistency.
//!
//! ## Record Types
//!
//! Catalog mutations are recorded in the WAL using three record types:
//!
//! 1. **CatalogWalRecord::CreateProcedure**: Records the creation of a new procedure.
//! 2. **CatalogWalRecord::AlterProcedure**: Records a procedure contract replacement (DEC-022).
//! 3. **CatalogWalRecord::DeprecateProcedure**: Records a procedure lifecycle transition.
//! 4. **CatalogWalRecord::DropProcedure**: Records a procedure removal (DEC-023).
//! 5. **CatalogWalRecord::ApplyCatalogVersion**: Records the application of a catalog version.
//!
//! ## Durability Semantics
//!
//! ### WAL-Before-Visible-Commit Doctrine
//!
//! All catalog mutations follow the WAL-before-visible-commit principle:
//!
//! - **Before Apply**: A `CatalogWalRecord::ApplyCatalogVersion` is written to the WAL
//!   **before** the mutation plan is applied to the in-memory catalog snapshot.
//! - **Durable Flush**: The WAL record is flushed to durable storage (disk or replica).
//! - **After Flush**: The catalog version is advanced and becomes visible to readers.
//! - **Recovery**: If a crash occurs before the flush completes, recovery discards the
//!   incomplete batch. If the flush completes but the snapshot is not updated, recovery
//!   uses the WAL record to rebuild the snapshot.
//!
//! ### Atomicity with CatalogVersion Bump
//!
//! The `CatalogWalRecord::ApplyCatalogVersion` includes:
//! - `batch_id`: The DefinitionBatchId for correlation with the batch manifest
//! - `version`: The new CatalogVersion being applied
//! - `record_count`: The number of individual operation records (Create, Alter, Deprecate, Drop)
//! - `lsn`: The LSN of this commit record in the WAL
//!
//! The version is **not visible** until:
//! 1. All `record_count` operation records have been durably written.
//! 2. The `ApplyCatalogVersion` record has been durably flushed.
//! 3. The in-memory catalog snapshot has been updated with the new version.
//!
//! ### Incomplete Record Recovery
//!
//! If recovery finds an incomplete batch (missing operation records or no commit record),
//! it **rejects** the batch:
//! - The batch ID is recorded in the recovery report.
//! - The incomplete mutations are **not** applied to the snapshot.
//! - The catalog version is **not** advanced.
//! - Readers continue to use the last durably committed version.
//!
//! ### LSN Correlation with DefinitionBatch
//!
//! Each `CatalogWalRecord::ApplyCatalogVersion` correlates to exactly one `DefinitionBatch`
//! through the `batch_id` field. Recovery uses the LSN from the WAL record to validate
//! that:
//! - The batch manifest entry references the correct batch ID and version.
//! - The WAL records form a complete, uninterrupted sequence for this batch.
//! - The LSN is monotonically increasing across batches.
//!
//! ## Recovery Integration
//!
//! ### Recovery Scanner Algorithm
//!
//! During startup, the recovery module scans the WAL and:
//!
//! 1. Reads all `CatalogWalRecord::ApplyCatalogVersion` records.
//! 2. For each version record:
//!    a. Verifies that `record_count` operation records precede it.
//!    b. Verifies that all operation records belong to the same `batch_id`.
//!    c. Verifies that no gaps or duplicates exist in the operation sequence.
//! 3. If all validations pass, replays the operations into the catalog snapshot.
//! 4. If any validation fails, records the batch as "incomplete" and skips replay.
//! 5. Advances the snapshot's `CatalogVersion` to the highest durably applied version.
//!
//! ### Manifest Correlation
//!
//! The recovery process also cross-checks with the catalog manifest:
//! - Each batch ID in the WAL must correspond to a manifest entry.
//! - The LSN range recorded in the manifest must match the WAL records.
//! - If a mismatch is found, the batch is marked as anomalous and skipped.
//!
//! ## Implementation Notes
//!
//! ### No Ad Hoc SQL or Unsafe Runtime Behavior
//!
//! - All operations are type-safe and validated during dry-run.
//! - No dynamic SQL is generated; all mutations use the typed `DefinitionBatch` model.
//! - No unsafe code is used in encoding/decoding.
//!
//! ### Crate Dependencies
//!
//! - `andromeda-storage` is used for WAL infrastructure but not inverted.
//! - Catalog WAL records are defined here; storage WAL records wrap them.
//! - Recovery correlation is bidirectional but encapsulated.
//!
//! ### Future Extensions
//!
//! Future WAL record types may include:
//! - `AlterTable`: Table schema evolution
//! - `DropTable`: Table removal with cascade/restrict policy
//! - `AlterStructuredObject`: Evolution of composite types
//! - `DropStructuredObject`: Type removal
//!
//! Each future type will follow the same durability semantics and recovery
//! integration pattern as defined here.

/// Design marker type (unit type, not emitted at runtime).
#[allow(missing_docs)]
pub struct CatalogWalRecordDesign;
