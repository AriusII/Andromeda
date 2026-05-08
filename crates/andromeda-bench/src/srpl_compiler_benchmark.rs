use std::time::Instant;

use andromeda_bench_harness::elapsed_micros;
use andromeda_srpl::procedure_compiler::{
    INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE, compile_narrow_procedure_signature_with_optimizer,
};

use crate::BenchmarkError;

pub const SRPL_COMPILE_OPTIMIZE_WORKLOAD_ID: &str = "srpl-compile-optimize-smoke";
pub const SRPL_COMPILE_OPTIMIZE_HARNESS_SOURCE: &str = "srpl-compiler-pipeline";
pub const SRPL_COMPILE_OPTIMIZE_HARNESS_NAME: &str =
    "compile_narrow_procedure_signature_with_optimizer";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplCompileOptimizeSmokeBenchmark {
    pub latencies_us: Vec<u64>,
    pub compiled_procedures: usize,
    pub optimized_procedures: usize,
    pub optimizer_diagnostics: usize,
}

pub fn run_srpl_compile_optimize_smoke_benchmark(
    samples: u32,
) -> Result<SrplCompileOptimizeSmokeBenchmark, BenchmarkError> {
    if samples == 0 {
        return Err(BenchmarkError::InsufficientSamplesForStatistics);
    }

    let mut latencies_us = Vec::with_capacity(samples as usize);
    let mut compiled_procedures = 0usize;
    let mut optimized_procedures = 0usize;
    let mut optimizer_diagnostics = 0usize;

    for _ in 0..samples {
        let started = Instant::now();
        let result = compile_narrow_procedure_signature_with_optimizer(
            INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE,
            Default::default(),
        )
        .map_err(|_| BenchmarkError::HarnessFailed)?;
        latencies_us.push(elapsed_micros(started));

        if result.original_ir.body.operations.is_empty()
            || result.optimized_ir.body.operations.is_empty()
            || result.phases.is_empty()
            || result
                .alternatives
                .iter()
                .filter(|plan| plan.chosen)
                .count()
                != 1
            || result.optimized_ir.result_streams.is_empty()
            || !result.chosen_cost.is_valid()
        {
            return Err(BenchmarkError::HarnessFailed);
        }

        compiled_procedures += 1;
        optimized_procedures += 1;
        optimizer_diagnostics += result.diagnostics.len();
    }

    Ok(SrplCompileOptimizeSmokeBenchmark {
        latencies_us,
        compiled_procedures,
        optimized_procedures,
        optimizer_diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srpl_compile_optimize_smoke_runs_real_compiler_pipeline() {
        let result = run_srpl_compile_optimize_smoke_benchmark(3).unwrap();

        assert_eq!(result.latencies_us.len(), 3);
        assert_eq!(result.compiled_procedures, 3);
        assert_eq!(result.optimized_procedures, 3);
        assert!(result.optimizer_diagnostics >= 3);
        assert!(result.latencies_us.iter().all(|latency| *latency >= 1));
    }

    #[test]
    fn srpl_compile_optimize_smoke_rejects_zero_samples() {
        assert_eq!(
            run_srpl_compile_optimize_smoke_benchmark(0).unwrap_err(),
            BenchmarkError::InsufficientSamplesForStatistics
        );
    }
}
