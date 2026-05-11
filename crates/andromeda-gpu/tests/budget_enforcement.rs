//! Verifies that `GpuBudget::fits` returns an error when any single budget
//! dimension is exceeded.

use andromeda_error::AndromedaErrorKind;
use andromeda_error::AndromedaResult;
use andromeda_gpu::{
    budget::{GpuBudget, GpuBudgetRequest},
    fallback::CpuFallback,
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
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

const BUDGET_MEMORY: u64 = 1_024;
const BUDGET_TIME: u64 = 100;
const BUDGET_TRANSFER: u64 = 512;

fn make_budget() -> GpuBudget {
    GpuBudget::new(BUDGET_MEMORY, BUDGET_TIME, BUDGET_TRANSFER).unwrap()
}

#[test]
fn request_exceeding_memory_bytes_returns_resource_error() {
    let budget = make_budget();
    let request = GpuBudgetRequest {
        memory_bytes: BUDGET_MEMORY + 1,
        time_ms: BUDGET_TIME,
        transfer_bytes: BUDGET_TRANSFER,
    };
    let err = budget.fits(&request).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Resource);
    assert!(
        err.message().contains("memory"),
        "error must mention 'memory': {}",
        err.message()
    );
}

#[test]
fn request_exceeding_time_ms_returns_resource_error() {
    let budget = make_budget();
    let request = GpuBudgetRequest {
        memory_bytes: BUDGET_MEMORY,
        time_ms: BUDGET_TIME + 1,
        transfer_bytes: BUDGET_TRANSFER,
    };
    let err = budget.fits(&request).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Resource);
    assert!(
        err.message().contains("time"),
        "error must mention 'time': {}",
        err.message()
    );
}

#[test]
fn request_exceeding_transfer_bytes_returns_resource_error() {
    let budget = make_budget();
    let request = GpuBudgetRequest {
        memory_bytes: BUDGET_MEMORY,
        time_ms: BUDGET_TIME,
        transfer_bytes: BUDGET_TRANSFER + 1,
    };
    let err = budget.fits(&request).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Resource);
    assert!(
        err.message().contains("transfer"),
        "error must mention 'transfer': {}",
        err.message()
    );
}

#[test]
fn request_at_exact_budget_limits_is_accepted() {
    let budget = make_budget();
    let request = GpuBudgetRequest {
        memory_bytes: BUDGET_MEMORY,
        time_ms: BUDGET_TIME,
        transfer_bytes: BUDGET_TRANSFER,
    };
    assert!(budget.fits(&request).is_ok());
}

#[test]
fn request_well_below_all_limits_is_accepted() {
    let budget = make_budget();
    let request = GpuBudgetRequest {
        memory_bytes: 1,
        time_ms: 1,
        transfer_bytes: 1,
    };
    assert!(budget.fits(&request).is_ok());
}

#[test]
fn budget_constructor_rejects_zero_limits() {
    assert!(GpuBudget::new(0, 100, 512).is_err());
    assert!(GpuBudget::new(1_024, 0, 512).is_err());
    assert!(GpuBudget::new(1_024, 100, 0).is_err());
}

struct StatsCpuFallback;

impl CpuFallback for StatsCpuFallback {
    type Input = StatsInput;
    type Output = Histogram;

    fn execute(&self, _input: &StatsInput) -> AndromedaResult<Histogram> {
        Ok(Histogram {
            buckets: vec![1, 2, 3],
        })
    }
}

struct AnalyticsCpuFallback;

impl CpuFallback for AnalyticsCpuFallback {
    type Input = AnalyticsInput;
    type Output = AnalyticsOutput;

    fn execute(&self, _input: &AnalyticsInput) -> AndromedaResult<AnalyticsOutput> {
        Ok(AnalyticsOutput {
            row_count: 10,
            digest: 20,
        })
    }
}

#[test]
fn stats_execute_over_budget_returns_resource_error_without_gpu_invocation() {
    let context = GpuStatsBindingContext {
        job: GpuJobClass::Statistics,
        kill_switch: KillSwitchHandle::new(),
        budget: GpuBudget::new(1, 1, 1).unwrap(),
        validation_gate: GpuStatsValidationGate::new(1.0).unwrap(),
        cpu_fallback: StatsCpuFallback,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };
    let input = StatsInput {
        snapshot_lsn: 7,
        column_id: 3,
    };
    let permit = StatsExecutionPermit::prevalidate(context.gpu_profile).unwrap();
    let gpu_called = Arc::new(AtomicBool::new(false));
    let gpu_called_for_closure = Arc::clone(&gpu_called);

    let err = context
        .execute(&permit, &input, move |_| {
            gpu_called_for_closure.store(true, Ordering::SeqCst);
            Ok(Histogram {
                buckets: vec![9, 9, 9],
            })
        })
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Resource);
    assert!(
        !gpu_called.load(Ordering::SeqCst),
        "GPU closure must not be invoked when request is over budget"
    );
}

#[test]
fn analytics_execute_over_budget_returns_resource_error_without_gpu_invocation() {
    let context = GpuAnalyticsBindingContext {
        job: GpuJobClass::Analytics,
        kill_switch: KillSwitchHandle::new(),
        budget: GpuBudget::new(1, 1, 1).unwrap(),
        cpu_fallback: AnalyticsCpuFallback,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };
    let input = AnalyticsInput {
        segment_id: 11,
        column_ids: vec![1, 2, 3],
    };
    let permit = AnalyticsExecutionPermit::prevalidate(context.gpu_profile).unwrap();
    let gpu_called = Arc::new(AtomicBool::new(false));
    let gpu_called_for_closure = Arc::clone(&gpu_called);

    let err = context
        .execute(&permit, &input, move |_| {
            gpu_called_for_closure.store(true, Ordering::SeqCst);
            Ok(AnalyticsOutput {
                row_count: 100,
                digest: 200,
            })
        })
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Resource);
    assert!(
        !gpu_called.load(Ordering::SeqCst),
        "GPU closure must not be invoked when request is over budget"
    );
}
