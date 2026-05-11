//! Verifies that an active kill switch causes `execute()` to short-circuit
//! without invoking the GPU closure or the CPU fallback.

use andromeda_error::AndromedaErrorKind;
use andromeda_error::AndromedaResult;
use andromeda_gpu::{
    budget::GpuBudget,
    fallback::CpuFallback,
    job::GpuJobClass,
    kill_switch::{KillReason, KillSwitchHandle},
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

struct PanicFallback;

impl CpuFallback for PanicFallback {
    type Input = StatsInput;
    type Output = Histogram;

    fn execute(&self, _input: &StatsInput) -> AndromedaResult<Histogram> {
        panic!("CPU fallback must not be invoked when kill switch is active");
    }
}

struct PanicAnalyticsFallback;

impl CpuFallback for PanicAnalyticsFallback {
    type Input = AnalyticsInput;
    type Output = AnalyticsOutput;

    fn execute(&self, _input: &AnalyticsInput) -> AndromedaResult<AnalyticsOutput> {
        panic!("CPU fallback must not be invoked when kill switch is active");
    }
}

#[test]
fn active_kill_switch_short_circuits_execute_with_resource_error() {
    let kill_switch = KillSwitchHandle::new();
    kill_switch.request_cancel(KillReason::OperatorRequested);

    let context = GpuStatsBindingContext {
        job: GpuJobClass::Statistics,
        kill_switch,
        budget: GpuBudget::new(1_024 * 1_024, 1_000, 1_024 * 1_024).unwrap(),
        validation_gate: GpuStatsValidationGate::new(1.0).unwrap(),
        cpu_fallback: PanicFallback,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };

    let input = StatsInput {
        snapshot_lsn: 1,
        column_id: 42,
    };
    let permit = StatsExecutionPermit::prevalidate(context.gpu_profile).unwrap();

    // GPU closure also must not be called.
    let result = context.execute(&permit, &input, |_| {
        panic!("GPU closure must not be invoked when kill switch is active")
    });

    let err = result.unwrap_err();
    assert_eq!(
        err.kind(),
        AndromedaErrorKind::Resource,
        "expected Resource error for cancellation, got {:?}",
        err.kind()
    );
    assert!(
        err.message().contains("cancel"),
        "error message must mention cancellation: {:?}",
        err.message()
    );
}

#[test]
fn inactive_kill_switch_does_not_block_execute() {
    struct SimpleFallback;

    impl CpuFallback for SimpleFallback {
        type Input = StatsInput;
        type Output = Histogram;

        fn execute(&self, _input: &StatsInput) -> AndromedaResult<Histogram> {
            Ok(Histogram {
                buckets: vec![10, 20, 30],
            })
        }
    }

    let kill_switch = KillSwitchHandle::new();
    // kill switch is Inactive

    let context = GpuStatsBindingContext {
        job: GpuJobClass::Statistics,
        kill_switch,
        budget: GpuBudget::new(1_024 * 1_024, 1_000, 1_024 * 1_024).unwrap(),
        validation_gate: GpuStatsValidationGate::new(5.0).unwrap(),
        cpu_fallback: SimpleFallback,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };

    let input = StatsInput {
        snapshot_lsn: 1,
        column_id: 0,
    };
    let permit = StatsExecutionPermit::prevalidate(context.gpu_profile).unwrap();

    let result = context.execute(&permit, &input, |_| {
        Ok(Histogram {
            buckets: vec![10, 20, 30],
        })
    });

    assert!(
        result.is_ok(),
        "inactive kill switch must not block execute"
    );
}

#[test]
fn analytics_active_kill_switch_short_circuits_execute_with_resource_error_and_no_gpu_invocation() {
    let kill_switch = KillSwitchHandle::new();
    kill_switch.request_cancel(KillReason::OperatorRequested);

    let context = GpuAnalyticsBindingContext {
        job: GpuJobClass::Analytics,
        kill_switch,
        budget: GpuBudget::new(1_024 * 1_024, 1_000, 1_024 * 1_024).unwrap(),
        cpu_fallback: PanicAnalyticsFallback,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };

    let input = AnalyticsInput {
        segment_id: 9,
        column_ids: vec![1, 2, 3],
    };
    let permit = AnalyticsExecutionPermit::prevalidate(context.gpu_profile).unwrap();
    let gpu_called = Arc::new(AtomicBool::new(false));
    let gpu_called_for_closure = Arc::clone(&gpu_called);

    let result = context.execute(&permit, &input, move |_| {
        gpu_called_for_closure.store(true, Ordering::SeqCst);
        Ok(AnalyticsOutput {
            row_count: 100,
            digest: 200,
        })
    });

    let err = result.unwrap_err();
    assert_eq!(
        err.kind(),
        AndromedaErrorKind::Resource,
        "expected Resource error for cancellation, got {:?}",
        err.kind()
    );
    assert!(
        err.message().contains("cancel"),
        "error message must mention cancellation: {:?}",
        err.message()
    );
    assert!(
        !gpu_called.load(Ordering::SeqCst),
        "GPU closure must not be invoked when kill switch is active"
    );
}
