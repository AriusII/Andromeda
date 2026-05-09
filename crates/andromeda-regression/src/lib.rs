#![forbid(unsafe_code)]

//! Advisory Andromeda benchmark regression analysis.
//!
//! This crate owns baseline comparison and regression reporting. Regression
//! reports are diagnostic evidence only and cannot drive optimizer, storage,
//! WAL, recovery, catalog, or security decisions by themselves.

mod regression_detection;

mod metric_math {
    pub(crate) use andromeda_bench_workload::{error_rate_ppm, percent_change};
}

pub use regression_detection::{
    BenchmarkBaseline, BenchmarkBaselineComparisonError, BenchmarkBaselineContext,
    RegressionAnalysis, RegressionReason,
};
