use crate::{
    BenchmarkError, BenchmarkEvidence, BenchmarkHardwareProfile, BenchmarkRunRequest,
    evaluate_budget,
};

pub fn run_bounded_benchmark(
    request: &BenchmarkRunRequest,
) -> Result<BenchmarkEvidence, BenchmarkError> {
    let workload = request.validate()?;
    let _profile = request.hardware_profile.materialize();

    let sample_count = request.samples;
    let profile_adjustment_us = match request.hardware_profile {
        BenchmarkHardwareProfile::Conservative => 0,
        BenchmarkHardwareProfile::DeclaredLocal => 1,
    };
    let workload_base_latency_us = match workload.id {
        "vertical-v0-smoke" => 2_500,
        "protocol-smoke-contract" => 1_000,
        "wal-append-smoke" => 1_500,
        "btree-lookup-smoke" => 25,
        "btree-range-scan-smoke" => 250,
        _ => return Err(BenchmarkError::UnknownWorkload),
    };

    let p50_latency_us = workload_base_latency_us
        + u64::from(request.samples)
        + u64::from(request.warmups)
        + profile_adjustment_us;
    let p95_latency_us = p50_latency_us * 2 + request.duration_ms / 1_000;
    let error_count = 0;
    let budget_status = evaluate_budget(
        workload,
        p50_latency_us,
        p95_latency_us,
        error_count,
        sample_count,
    )?;
    let requested_iterations = u64::from(request.samples) + u64::from(request.warmups);
    let elapsed_ms = request.duration_ms.min(requested_iterations.max(1));

    Ok(BenchmarkEvidence {
        workload_id: workload.id.to_string(),
        hardware_profile: request.hardware_profile,
        duration_ms: request.duration_ms,
        samples: request.samples,
        warmups: request.warmups,
        started_at_unix_ms: 0,
        elapsed_ms,
        sample_count,
        p50_latency_us,
        p95_latency_us,
        error_count,
        budget_status,
        diagnostic_only: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BudgetStatus;

    #[test]
    fn bounded_runner_produces_deterministic_diagnostic_evidence() {
        let mut request = BenchmarkRunRequest::new("protocol-smoke-contract");
        request.duration_ms = 1_000;
        request.samples = 5;
        request.warmups = 1;
        request.hardware_profile = BenchmarkHardwareProfile::DeclaredLocal;

        let first = run_bounded_benchmark(&request).unwrap();
        let second = run_bounded_benchmark(&request).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.started_at_unix_ms, 0);
        assert_eq!(first.elapsed_ms, 6);
        assert_eq!(first.sample_count, 5);
        assert_eq!(first.budget_status, BudgetStatus::Passed);
        assert!(first.diagnostic_only);
    }
}
