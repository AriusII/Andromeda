//! Verifies that a GPU histogram that diverges from the CPU shadow beyond the
//! validation threshold is rejected and the CPU result is returned instead.

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

/// CPU shadow always returns a fixed authoritative histogram.
struct AuthoritativeCpuFallback;

impl CpuFallback for AuthoritativeCpuFallback {
    type Input = StatsInput;
    type Output = Histogram;

    fn execute(&self, _input: &StatsInput) -> AndromedaResult<Histogram> {
        Ok(Histogram {
            buckets: vec![100, 200, 300],
        })
    }
}

#[test]
fn divergent_gpu_histogram_is_rejected_and_cpu_result_is_returned() {
    // threshold = 1%; GPU bucket[0] = 103 vs CPU = 100 → 3% deviation → rejected
    let context = GpuStatsBindingContext {
        job: GpuJobClass::Statistics,
        kill_switch: KillSwitchHandle::new(),
        budget: GpuBudget::new(1_024 * 1_024, 1_000, 1_024 * 1_024).unwrap(),
        validation_gate: GpuStatsValidationGate::new(1.0).unwrap(),
        cpu_fallback: AuthoritativeCpuFallback,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };

    let input = StatsInput {
        snapshot_lsn: 1,
        column_id: 0,
    };
    let permit = StatsExecutionPermit::prevalidate(context.gpu_profile).unwrap();

    let divergent_gpu_histogram = Histogram {
        buckets: vec![103, 200, 300], // bucket[0]: 3% deviation > 1% threshold
    };

    let (returned_histogram, trace) = context
        .execute(&permit, &input, |_| Ok(divergent_gpu_histogram.clone()))
        .unwrap();

    // The returned histogram must be the CPU result, not the GPU one.
    let cpu_expected = Histogram {
        buckets: vec![100, 200, 300],
    };
    assert_eq!(
        returned_histogram, cpu_expected,
        "divergent GPU output must be replaced by CPU shadow"
    );

    // Trace must show validation was rejected.
    assert!(
        matches!(trace.validation_state, ValidationState::Rejected(_)),
        "validation_state must be Rejected, got {:?}",
        trace.validation_state
    );

    // Trace must record the fallback reason.
    assert_eq!(
        trace.fallback_reason,
        Some(FallbackReason::CpuValidationFailed),
        "fallback_reason must be CpuValidationFailed"
    );

    // The returned histogram must never be the divergent GPU one.
    assert_ne!(
        returned_histogram, divergent_gpu_histogram,
        "divergent GPU histogram must not be published"
    );
}

#[test]
fn matching_gpu_histogram_is_validated_and_returned() {
    // When GPU and CPU agree within threshold, GPU result is returned.
    let context = GpuStatsBindingContext {
        job: GpuJobClass::Statistics,
        kill_switch: KillSwitchHandle::new(),
        budget: GpuBudget::new(1_024 * 1_024, 1_000, 1_024 * 1_024).unwrap(),
        validation_gate: GpuStatsValidationGate::new(1.0).unwrap(),
        cpu_fallback: AuthoritativeCpuFallback,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };

    let input = StatsInput {
        snapshot_lsn: 1,
        column_id: 0,
    };
    let permit = StatsExecutionPermit::prevalidate(context.gpu_profile).unwrap();

    // GPU matches CPU exactly.
    let (returned_histogram, trace) = context
        .execute(&permit, &input, |_| {
            Ok(Histogram {
                buckets: vec![100, 200, 300],
            })
        })
        .unwrap();

    assert_eq!(
        returned_histogram,
        Histogram {
            buckets: vec![100, 200, 300]
        }
    );
    assert_eq!(
        trace.validation_state,
        ValidationState::Validated,
        "matching GPU output must be Validated"
    );
    assert_eq!(
        trace.fallback_reason, None,
        "no fallback reason for validated result"
    );
}
