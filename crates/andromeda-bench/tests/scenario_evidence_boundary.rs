#![forbid(unsafe_code)]

//! Benchmark history to ScenarioEvidence boundary integration tests.

#[path = "scenario_evidence_boundary/advisory_export.rs"]
mod advisory_export;
#[path = "scenario_evidence_boundary/benchmark_run.rs"]
mod benchmark_run;
#[path = "scenario_evidence_boundary/common.rs"]
mod common;
#[path = "scenario_evidence_boundary/history_rejections.rs"]
mod history_rejections;
#[path = "scenario_evidence_boundary/target_validity.rs"]
mod target_validity;
