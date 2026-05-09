//! MVCC reclamation contract tests.
//!
//! These tests verify that reclamation marks are created only for versions that
//! are invisible to every active snapshot and whose creator has durable
//! committed status.

#[path = "mvcc_reclamation_contract/support.rs"]
mod support;

#[path = "mvcc_reclamation_contract/batch_scenarios.rs"]
mod batch_scenarios;
#[path = "mvcc_reclamation_contract/eligibility.rs"]
mod eligibility;
#[path = "mvcc_reclamation_contract/mark_creation.rs"]
mod mark_creation;
#[path = "mvcc_reclamation_contract/visibility_boundaries.rs"]
mod visibility_boundaries;
