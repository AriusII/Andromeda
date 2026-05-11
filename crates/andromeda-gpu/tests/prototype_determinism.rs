//! Determinism checks for GPU prototype binding contexts.

use andromeda_error::AndromedaResult;
use andromeda_gpu::{
    budget::GpuBudget,
    fallback::{CpuFallback, ValidationState},
    job::GpuJobClass,
    kill_switch::KillSwitchHandle,
    policy::GpuProfile,
    prototypes::{
        analytics::{AnalyticsInput, AnalyticsOutput, GpuAnalyticsBindingContext},
        permit::{AnalyticsExecutionPermit, StatsExecutionPermit},
        stats::{GpuStatsBindingContext, StatsInput},
    },
    validation::{GpuStatsValidationGate, Histogram},
};

struct DeterministicStatsCpuFallback;

impl CpuFallback for DeterministicStatsCpuFallback {
    type Input = StatsInput;
    type Output = Histogram;

    fn execute(&self, _input: &StatsInput) -> AndromedaResult<Histogram> {
        Ok(Histogram {
            buckets: vec![0, 0, 0],
        })
    }
}

struct DeterministicAnalyticsCpuFallback;

impl CpuFallback for DeterministicAnalyticsCpuFallback {
    type Input = AnalyticsInput;
    type Output = AnalyticsOutput;

    fn execute(&self, _input: &AnalyticsInput) -> AndromedaResult<AnalyticsOutput> {
        Ok(AnalyticsOutput {
            row_count: 0,
            digest: 0,
        })
    }
}

#[test]
fn stats_repeated_identical_input_produces_identical_output_and_validation_state() {
    let context = GpuStatsBindingContext {
        job: GpuJobClass::Statistics,
        kill_switch: KillSwitchHandle::new(),
        budget: GpuBudget::new(1_024 * 1_024, 1_000, 1_024 * 1_024).unwrap(),
        validation_gate: GpuStatsValidationGate::new(0.0).unwrap(),
        cpu_fallback: DeterministicStatsCpuFallback,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };
    let input = StatsInput {
        snapshot_lsn: 9,
        column_id: 3,
    };
    let permit = StatsExecutionPermit::prevalidate(context.gpu_profile).unwrap();

    let (first_output, first_trace) = context
        .execute(&permit, &input, |_| {
            Ok(Histogram {
                buckets: vec![0, 0, 0],
            })
        })
        .unwrap();
    let (second_output, second_trace) = context
        .execute(&permit, &input, |_| {
            Ok(Histogram {
                buckets: vec![0, 0, 0],
            })
        })
        .unwrap();

    assert_eq!(first_output, second_output);
    assert_eq!(first_trace.validation_state, second_trace.validation_state);
    assert_eq!(first_trace.validation_state, ValidationState::Validated);
    assert_eq!(first_trace.fallback_reason, None);
    assert_eq!(first_trace, second_trace);
}

#[test]
fn analytics_repeated_identical_input_with_empty_columns_is_deterministic() {
    let context = GpuAnalyticsBindingContext {
        job: GpuJobClass::Analytics,
        kill_switch: KillSwitchHandle::new(),
        budget: GpuBudget::new(1_024 * 1_024, 1_000, 1_024 * 1_024).unwrap(),
        cpu_fallback: DeterministicAnalyticsCpuFallback,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };
    let edge_input = AnalyticsInput {
        segment_id: 0,
        column_ids: Vec::new(),
    };
    let permit = AnalyticsExecutionPermit::prevalidate(context.gpu_profile).unwrap();

    let (first_output, first_trace) = context
        .execute(&permit, &edge_input, |_| {
            Ok(AnalyticsOutput {
                row_count: 0,
                digest: 0,
            })
        })
        .unwrap();
    let (second_output, second_trace) = context
        .execute(&permit, &edge_input, |_| {
            Ok(AnalyticsOutput {
                row_count: 0,
                digest: 0,
            })
        })
        .unwrap();

    assert_eq!(first_output, second_output);
    assert_eq!(first_trace.validation_state, second_trace.validation_state);
    assert_eq!(first_trace.validation_state, ValidationState::Validated);
    assert_eq!(first_trace.fallback_reason, None);
    assert_eq!(first_trace, second_trace);
}
