//! End-to-end tests for the IAM pipeline.
//!
//! These tests verify the complete authorization flow:
//! - Principal resolution from certificate fingerprint
//! - Permission matrix evaluation per role
//! - RBAC authorization decisions
//! - Audit event emission and correlation
//! - Integration with admission control

#[path = "iam_pipeline_e2e/support.rs"]
mod support;

#[path = "iam_pipeline_e2e/admission_audit.rs"]
mod admission_audit;
#[path = "iam_pipeline_e2e/edge_cases.rs"]
mod edge_cases;
#[path = "iam_pipeline_e2e/permission_evaluation.rs"]
mod permission_evaluation;
#[path = "iam_pipeline_e2e/permission_matching.rs"]
mod permission_matching;
#[path = "iam_pipeline_e2e/principal_resolution.rs"]
mod principal_resolution;
#[path = "iam_pipeline_e2e/resolver_state.rs"]
mod resolver_state;
#[path = "iam_pipeline_e2e/role_permissions.rs"]
mod role_permissions;
