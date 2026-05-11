//! Verifies GPU compute failures map to a precise fallback reason distinct from
//! device unavailability and CPU validation rejection.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_gpu::{
    budget::GpuBudget,
    fallback::{CpuFallback, ValidationState},
    job::{FallbackReason, GpuJobClass},
    kill_switch::KillSwitchHandle,
    policy::GpuProfile,
    prototypes::{
        analytics::{AnalyticsInput, AnalyticsOutput, GpuAnalyticsBindingContext},
        permit::{AnalyticsExecutionPermit, StatsExecutionPermit},
        stats::{GpuStatsBindingContext, StatsInput},
    },
    validation::{GpuStatsValidationGate, Histogram},
};

struct StatsCpu;

impl CpuFallback for StatsCpu {
    type Input = StatsInput;
    type Output = Histogram;

    fn execute(&self, _input: &StatsInput) -> AndromedaResult<Histogram> {
        Ok(Histogram {
            buckets: vec![100, 200, 300],
        })
    }
}

struct AnalyticsCpu;

impl CpuFallback for AnalyticsCpu {
    type Input = AnalyticsInput;
    type Output = AnalyticsOutput;

    fn execute(&self, _input: &AnalyticsInput) -> AndromedaResult<AnalyticsOutput> {
        Ok(AnalyticsOutput {
            row_count: 1_000,
            digest: 42,
        })
    }
}

#[test]
fn stats_gpu_compute_error_uses_gpu_computation_failed_reason() {
    let context = GpuStatsBindingContext {
        job: GpuJobClass::Statistics,
        kill_switch: KillSwitchHandle::new(),
        budget: GpuBudget::new(1_024 * 1_024, 1_000, 1_024 * 1_024).unwrap(),
        validation_gate: GpuStatsValidationGate::new(1.0).unwrap(),
        cpu_fallback: StatsCpu,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };
    let input = StatsInput {
        snapshot_lsn: 1,
        column_id: 7,
    };
    let permit = StatsExecutionPermit::prevalidate(context.gpu_profile).unwrap();

    let (result, trace) = context
        .execute(&permit, &input, |_| {
            Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "simulated GPU compute failure",
            ))
        })
        .unwrap();

    assert_eq!(
        result,
        Histogram {
            buckets: vec![100, 200, 300]
        }
    );
    assert_eq!(trace.validation_state, ValidationState::NotValidated);
    assert_eq!(
        trace.fallback_reason,
        Some(FallbackReason::GpuComputationFailed)
    );
}

#[test]
fn analytics_gpu_compute_error_uses_gpu_computation_failed_reason() {
    let context = GpuAnalyticsBindingContext {
        job: GpuJobClass::Analytics,
        kill_switch: KillSwitchHandle::new(),
        budget: GpuBudget::new(1_024 * 1_024, 1_000, 1_024 * 1_024).unwrap(),
        cpu_fallback: AnalyticsCpu,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };
    let input = AnalyticsInput {
        segment_id: 5,
        column_ids: vec![1, 2, 3],
    };
    let permit = AnalyticsExecutionPermit::prevalidate(context.gpu_profile).unwrap();

    let (result, trace) = context
        .execute(&permit, &input, |_| {
            Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "simulated GPU analytics failure",
            ))
        })
        .unwrap();

    assert_eq!(
        result,
        AnalyticsOutput {
            row_count: 1_000,
            digest: 42
        }
    );
    assert_eq!(trace.validation_state, ValidationState::NotValidated);
    assert_eq!(
        trace.fallback_reason,
        Some(FallbackReason::GpuComputationFailed)
    );
}

#[test]
fn device_unavailable_still_maps_to_device_unavailable() {
    let context = GpuStatsBindingContext {
        job: GpuJobClass::Statistics,
        kill_switch: KillSwitchHandle::new(),
        budget: GpuBudget::new(1_024 * 1_024, 1_000, 1_024 * 1_024).unwrap(),
        validation_gate: GpuStatsValidationGate::new(1.0).unwrap(),
        cpu_fallback: StatsCpu,
        gpu_profile: GpuProfile::disabled(),
    };
    let input = StatsInput {
        snapshot_lsn: 1,
        column_id: 0,
    };
    let permit = StatsExecutionPermit::prevalidate(context.gpu_profile).unwrap();

    let (_, trace) = context
        .execute(&permit, &input, |_| {
            panic!("GPU closure must not run when unavailable")
        })
        .unwrap();

    assert_eq!(
        trace.fallback_reason,
        Some(FallbackReason::DeviceUnavailable)
    );
    assert_eq!(trace.validation_state, ValidationState::NotValidated);
}

#[test]
fn cpu_validation_failure_still_maps_to_cpu_validation_failed() {
    let context = GpuStatsBindingContext {
        job: GpuJobClass::Statistics,
        kill_switch: KillSwitchHandle::new(),
        budget: GpuBudget::new(1_024 * 1_024, 1_000, 1_024 * 1_024).unwrap(),
        validation_gate: GpuStatsValidationGate::new(1.0).unwrap(),
        cpu_fallback: StatsCpu,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };
    let input = StatsInput {
        snapshot_lsn: 2,
        column_id: 3,
    };
    let permit = StatsExecutionPermit::prevalidate(context.gpu_profile).unwrap();

    let (_, trace) = context
        .execute(&permit, &input, |_| {
            Ok(Histogram {
                buckets: vec![105, 200, 300],
            })
        })
        .unwrap();

    assert!(matches!(
        trace.validation_state,
        ValidationState::Rejected(_)
    ));
    assert_eq!(
        trace.fallback_reason,
        Some(FallbackReason::CpuValidationFailed)
    );
}
