//! Verifies that when `GpuProfile::available` is `false`, the GPU closure is
//! never invoked and the CPU fallback result is returned instead.
//!
//! The GPU closure is a panic to prove that it is not called.

use andromeda_error::AndromedaResult;
use andromeda_gpu::{
    budget::GpuBudget,
    fallback::{CpuFallback, ValidationState},
    job::{FallbackReason, GpuJobClass},
    kill_switch::KillSwitchHandle,
    policy::GpuProfile,
    prototypes::{
        permit::StatsExecutionPermit,
        stats::{GpuStatsBindingContext, StatsInput},
    },
    validation::{GpuStatsValidationGate, Histogram},
};

struct AuthoritativeCpuFallback;

impl CpuFallback for AuthoritativeCpuFallback {
    type Input = StatsInput;
    type Output = Histogram;

    fn execute(&self, _input: &StatsInput) -> AndromedaResult<Histogram> {
        Ok(Histogram {
            buckets: vec![42, 84, 168],
        })
    }
}

#[test]
fn gpu_absent_returns_cpu_fallback_without_invoking_gpu_closure() {
    let context = GpuStatsBindingContext {
        job: GpuJobClass::Statistics,
        kill_switch: KillSwitchHandle::new(),
        budget: GpuBudget::new(1_024 * 1_024, 1_000, 1_024 * 1_024).unwrap(),
        validation_gate: GpuStatsValidationGate::new(1.0).unwrap(),
        cpu_fallback: AuthoritativeCpuFallback,
        gpu_profile: GpuProfile::disabled(), // GPU not available
    };

    let input = StatsInput {
        snapshot_lsn: 0,
        column_id: 0,
    };
    let permit = StatsExecutionPermit::prevalidate(context.gpu_profile).unwrap();

    // GPU closure is a panic — it must NEVER be called when GPU is absent.
    let (returned_histogram, trace) = context
        .execute(&permit, &input, |_| {
            panic!("GPU closure must not be invoked when GPU is absent")
        })
        .unwrap();

    // Result must match what the CPU fallback produces.
    let cpu_expected = AuthoritativeCpuFallback.execute(&input).unwrap();
    assert_eq!(
        returned_histogram, cpu_expected,
        "result must be the CPU fallback output"
    );

    // Trace must record the device unavailability.
    assert_eq!(
        trace.fallback_reason,
        Some(FallbackReason::DeviceUnavailable),
        "trace must record DeviceUnavailable fallback reason"
    );

    // Validation state is NotValidated because we never ran the GPU.
    assert_eq!(
        trace.validation_state,
        ValidationState::NotValidated,
        "GPU-absent path must leave validation_state as NotValidated"
    );
}

#[test]
fn gpu_disabled_profile_means_profile_available_is_false() {
    let profile = GpuProfile::disabled();
    assert!(
        !profile.available,
        "GpuProfile::disabled() must have available=false"
    );
}
