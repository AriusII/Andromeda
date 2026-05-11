//! Permit-boundary tests for prototype stats/analytics execution.

use andromeda_error::AndromedaResult;
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
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct StatsCpu;
impl CpuFallback for StatsCpu {
    type Input = StatsInput;
    type Output = Histogram;

    fn execute(&self, _input: &StatsInput) -> AndromedaResult<Histogram> {
        Ok(Histogram {
            buckets: vec![11, 22, 33],
        })
    }
}

struct AnalyticsCpu;
impl CpuFallback for AnalyticsCpu {
    type Input = AnalyticsInput;
    type Output = AnalyticsOutput;

    fn execute(&self, _input: &AnalyticsInput) -> AndromedaResult<AnalyticsOutput> {
        Ok(AnalyticsOutput {
            row_count: 77,
            digest: 88,
        })
    }
}

#[test]
fn stats_permit_disabled_profile_forces_cpu_path() {
    let context = GpuStatsBindingContext {
        job: GpuJobClass::Statistics,
        kill_switch: KillSwitchHandle::new(),
        budget: GpuBudget::new(1_024 * 1_024, 1_000, 1_024 * 1_024).unwrap(),
        validation_gate: GpuStatsValidationGate::new(1.0).unwrap(),
        cpu_fallback: StatsCpu,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };
    let permit = StatsExecutionPermit::prevalidate(GpuProfile::disabled()).unwrap();
    let input = StatsInput {
        snapshot_lsn: 9,
        column_id: 1,
    };
    let gpu_called = Arc::new(AtomicBool::new(false));
    let gpu_called_for_closure = Arc::clone(&gpu_called);

    let (output, trace) = context
        .execute(&permit, &input, move |_| {
            gpu_called_for_closure.store(true, Ordering::SeqCst);
            Ok(Histogram {
                buckets: vec![1, 2, 3],
            })
        })
        .unwrap();

    assert_eq!(
        output,
        Histogram {
            buckets: vec![11, 22, 33]
        }
    );
    assert!(!gpu_called.load(Ordering::SeqCst));
    assert_eq!(
        trace.fallback_reason,
        Some(FallbackReason::DeviceUnavailable)
    );
    assert_eq!(trace.validation_state, ValidationState::NotValidated);
}

#[test]
fn analytics_permit_disabled_profile_forces_cpu_path() {
    let context = GpuAnalyticsBindingContext {
        job: GpuJobClass::Analytics,
        kill_switch: KillSwitchHandle::new(),
        budget: GpuBudget::new(1_024 * 1_024, 1_000, 1_024 * 1_024).unwrap(),
        cpu_fallback: AnalyticsCpu,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };
    let permit = AnalyticsExecutionPermit::prevalidate(GpuProfile::disabled()).unwrap();
    let input = AnalyticsInput {
        segment_id: 3,
        column_ids: vec![4, 5],
    };
    let gpu_called = Arc::new(AtomicBool::new(false));
    let gpu_called_for_closure = Arc::clone(&gpu_called);

    let (output, trace) = context
        .execute(&permit, &input, move |_| {
            gpu_called_for_closure.store(true, Ordering::SeqCst);
            Ok(AnalyticsOutput {
                row_count: 1,
                digest: 2,
            })
        })
        .unwrap();

    assert_eq!(
        output,
        AnalyticsOutput {
            row_count: 77,
            digest: 88
        }
    );
    assert!(!gpu_called.load(Ordering::SeqCst));
    assert_eq!(
        trace.fallback_reason,
        Some(FallbackReason::DeviceUnavailable)
    );
    assert_eq!(trace.validation_state, ValidationState::NotValidated);
}
