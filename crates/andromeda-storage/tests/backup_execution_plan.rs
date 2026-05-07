//! Physical backup execution plan contract tests.
//!
//! The test target is split by backup/recovery contract family:
//!
//! - manifest versioning, checksums, and bounded payloads
//! - physical plan metadata and extent accounting
//! - PITR artifact round-trip from cold snapshot plus durable WAL
//! - WAL archive range coverage and evidence
//! - compatibility and failure modes for artifact validation

#[path = "backup_execution_plan/compatibility_failure_modes.rs"]
mod compatibility_failure_modes;
#[path = "backup_execution_plan/manifest.rs"]
mod manifest;
#[path = "backup_execution_plan/physical_plan.rs"]
mod physical_plan;
#[path = "backup_execution_plan/pitr.rs"]
mod pitr;
#[path = "backup_execution_plan/support.rs"]
mod support;
#[path = "backup_execution_plan/wal_archive.rs"]
mod wal_archive;
