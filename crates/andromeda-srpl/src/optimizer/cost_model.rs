use crate::{Cardinality, SrplBusinessOperationKindIr, SrplProcedureIr};

/// CPU cost per tuple (normalised units).
pub const CPU_TUPLE_COST: f64 = 0.5;
/// I/O cost per page (normalised units; ~20× CPU reflects SSD latency).
pub const IO_PAGE_COST: f64 = 10.0;
/// Memory cost per page (normalised units).
pub const MEMORY_PAGE_COST: f64 = 2.0;
/// Assumed tuples per storage page.
pub const ASSUMED_PAGE_SIZE_TUPLES: f64 = 100.0;

// Default selectivities when no histogram is available.
const DEFAULT_EQUALITY_SELECTIVITY: f64 = 0.10;
// Default row counts when no stats are available.
const DEFAULT_ROW_COUNT_ONE: f64 = 1.0;
const DEFAULT_ROW_COUNT_OPTIONAL_ONE: f64 = 0.5;
const DEFAULT_ROW_COUNT_NON_EMPTY_MANY: f64 = 10.0;
const DEFAULT_ROW_COUNT_MANY: f64 = 100.0;

/// Cost accuracy threshold above which a warning trace is emitted.
pub const COST_ACCURACY_WARN_THRESHOLD: f64 = 0.50;
/// Cost accuracy threshold above which a stats-reanalyze recommendation is set.
pub const COST_ACCURACY_ALERT_THRESHOLD: f64 = 2.00;

/// Bounded cost estimate produced by the `CostModel` pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CostEstimate {
    pub cpu_cost: f64,
    pub io_cost: f64,
    pub memory_cost: f64,
    pub total_cost: f64,
}

impl CostEstimate {
    const COMPONENT_CONSISTENCY_TOLERANCE: f64 = 0.000_001;

    /// A zero-cost estimate (used as a baseline / default).
    pub const fn zero() -> Self {
        Self {
            cpu_cost: 0.0,
            io_cost: 0.0,
            memory_cost: 0.0,
            total_cost: 0.0,
        }
    }

    /// True when all components are non-negative and the total is consistent.
    pub fn is_valid(&self) -> bool {
        if !self.cpu_cost.is_finite()
            || !self.io_cost.is_finite()
            || !self.memory_cost.is_finite()
            || !self.total_cost.is_finite()
        {
            return false;
        }

        if self.cpu_cost < 0.0
            || self.io_cost < 0.0
            || self.memory_cost < 0.0
            || self.total_cost < 0.0
        {
            return false;
        }

        let component_total = self.cpu_cost + self.io_cost + self.memory_cost;
        (component_total - self.total_cost).abs() <= Self::COMPONENT_CONSISTENCY_TOLERANCE
    }
}

/// Actual measured cost after an invocation completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActualCost {
    pub cpu_nanos: u64,
    pub io_pages: u64,
    pub memory_pages: u64,
}

/// Estimate the cost of a procedure without statistics.
///
/// Uses default row counts and selectivities. Always returns a valid,
/// non-negative, non-NaN estimate.
pub fn estimate_without_stats(ir: &SrplProcedureIr) -> CostEstimate {
    let mut total_tuples = 0.0_f64;
    let mut total_io = 0.0_f64;
    let mut total_mem = 0.0_f64;

    for op in &ir.body.operations {
        match &op.kind {
            SrplBusinessOperationKindIr::Read {
                cardinality,
                predicates,
                ..
            } => {
                let base_rows = default_rows(*cardinality);
                let sel = default_conjunct_selectivity(predicates.len());
                let tuples = (base_rows * sel).max(0.0);
                let pages = (tuples / ASSUMED_PAGE_SIZE_TUPLES).ceil().max(1.0);
                total_tuples += tuples;
                total_io += pages;
                total_mem += pages;
            }
            SrplBusinessOperationKindIr::Update {
                affected_rows_exact,
                ..
            } => {
                let rows = affected_rows_exact.unwrap_or(1) as f64;
                let pages = (rows / ASSUMED_PAGE_SIZE_TUPLES).ceil().max(1.0);
                total_tuples += rows;
                total_io += pages * 2.0; // read + write-back
                total_mem += pages;
            }
            // Emit, Assert, Raise: no storage cost.
            _ => {}
        }
    }

    let cpu = total_tuples * CPU_TUPLE_COST;
    let io = total_io * IO_PAGE_COST;
    let mem = total_mem * MEMORY_PAGE_COST;
    let total = cpu + io + mem;

    CostEstimate {
        cpu_cost: cpu,
        io_cost: io,
        memory_cost: mem,
        total_cost: total.max(0.0),
    }
}

/// Compute cost estimation error ratio: `|actual - estimated| / estimated`.
///
/// Returns 0.0 when the estimated cost is zero (no divide-by-zero).
pub fn cost_accuracy(estimated: &CostEstimate, actual: &ActualCost) -> f64 {
    // Convert actual to the same normalised units as the estimate.
    let actual_cpu_cost = (actual.cpu_nanos as f64 / 1_000_000.0) * CPU_TUPLE_COST;
    let actual_io_cost = actual.io_pages as f64 * IO_PAGE_COST;
    let actual_mem_cost = actual.memory_pages as f64 * MEMORY_PAGE_COST;
    let actual_total = actual_cpu_cost + actual_io_cost + actual_mem_cost;

    if estimated.total_cost < f64::EPSILON {
        return 0.0;
    }
    (actual_total - estimated.total_cost).abs() / estimated.total_cost
}

fn default_rows(c: Cardinality) -> f64 {
    match c {
        Cardinality::One => DEFAULT_ROW_COUNT_ONE,
        Cardinality::OptionalOne => DEFAULT_ROW_COUNT_OPTIONAL_ONE,
        Cardinality::NonEmptyMany => DEFAULT_ROW_COUNT_NON_EMPTY_MANY,
        Cardinality::Many => DEFAULT_ROW_COUNT_MANY,
    }
}

fn default_conjunct_selectivity(predicate_count: usize) -> f64 {
    if predicate_count == 0 {
        return 1.0;
    }
    // Each equality predicate independently contributes DEFAULT_EQUALITY_SELECTIVITY.
    DEFAULT_EQUALITY_SELECTIVITY.powi(predicate_count as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Cardinality, SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplProcedureBodyIr,
        SrplProcedureIr,
    };
    use andromeda_catalog::QualifiedName;

    fn qn(s: &str) -> QualifiedName {
        QualifiedName::parse(s).unwrap()
    }

    fn read_ir(cardinality: Cardinality) -> SrplProcedureIr {
        SrplProcedureIr {
            name: qn("db.ns.P"),
            inputs: vec![],
            result_streams: vec![],
            body: SrplProcedureBodyIr {
                operations: vec![SrplBusinessOperationIr {
                    ordinal: 0,
                    kind: SrplBusinessOperationKindIr::Read {
                        source: qn("db.ns.T"),
                        binding: "T".into(),
                        cardinality,
                        predicates: vec![],
                    },
                }],
            },
        }
    }

    // T-CM-01
    #[test]
    fn single_read_cardinality_one_no_predicates() {
        let ir = read_ir(Cardinality::One);
        let cost = estimate_without_stats(&ir);
        assert!(cost.is_valid());
        // 1 tuple × 0.5 + 1 page × 10.0 + 1 page × 2.0 = 12.5
        assert!(
            (cost.total_cost - 12.5).abs() < 0.001,
            "got {}",
            cost.total_cost
        );
    }

    // T-CM-02
    #[test]
    fn single_read_cardinality_many_no_predicates() {
        let ir = read_ir(Cardinality::Many);
        let cost = estimate_without_stats(&ir);
        assert!(cost.is_valid());
        // 100 tuples × 0.5 + 1 page × 10.0 + 1 page × 2.0 = 62.0
        assert!(
            (cost.total_cost - 62.0).abs() < 0.001,
            "got {}",
            cost.total_cost
        );
    }

    // T-CM-08: zero-cost accuracy is safe
    #[test]
    fn cost_accuracy_zero_estimate_returns_zero() {
        let zero_estimate = CostEstimate::zero();
        let actual = ActualCost {
            cpu_nanos: 1000,
            io_pages: 1,
            memory_pages: 1,
        };
        assert_eq!(cost_accuracy(&zero_estimate, &actual), 0.0);
    }

    #[test]
    fn estimate_is_always_valid_for_empty_body() {
        let ir = SrplProcedureIr {
            name: qn("db.ns.P"),
            inputs: vec![],
            result_streams: vec![],
            body: SrplProcedureBodyIr { operations: vec![] },
        };
        let cost = estimate_without_stats(&ir);
        assert!(cost.is_valid());
        assert_eq!(cost.total_cost, 0.0);
    }

    #[test]
    fn cost_estimate_rejects_invalid_or_inconsistent_components() {
        assert!(
            !CostEstimate {
                cpu_cost: f64::NAN,
                io_cost: 0.0,
                memory_cost: 0.0,
                total_cost: 0.0,
            }
            .is_valid()
        );
        assert!(
            !CostEstimate {
                cpu_cost: 1.0,
                io_cost: f64::INFINITY,
                memory_cost: 0.0,
                total_cost: 1.0,
            }
            .is_valid()
        );
        assert!(
            !CostEstimate {
                cpu_cost: -1.0,
                io_cost: 0.0,
                memory_cost: 0.0,
                total_cost: 0.0,
            }
            .is_valid()
        );
        assert!(
            !CostEstimate {
                cpu_cost: 1.0,
                io_cost: 1.0,
                memory_cost: 1.0,
                total_cost: 2.0,
            }
            .is_valid()
        );
    }

    // T-CM-06 / T-CM-07: cost accuracy thresholds
    #[test]
    fn cost_accuracy_flags_high_error() {
        // Estimated cost of ~12.5, actual much higher → error > 50%
        let ir = read_ir(Cardinality::One);
        let estimate = estimate_without_stats(&ir);
        // Simulate actual = 2× the estimate via io_pages manipulation
        let actual = ActualCost {
            cpu_nanos: 0,
            io_pages: 100, // 100 × 10.0 = 1000 io cost alone
            memory_pages: 0,
        };
        let ratio = cost_accuracy(&estimate, &actual);
        assert!(
            ratio > COST_ACCURACY_WARN_THRESHOLD,
            "expected ratio > {}, got {}",
            COST_ACCURACY_WARN_THRESHOLD,
            ratio
        );
    }

    #[test]
    fn estimate_is_non_negative_for_any_operation_mix() {
        use andromeda_srpl_ir::{SrplAssignmentIr, SrplValueIr};
        let ir = SrplProcedureIr {
            name: qn("db.ns.P"),
            inputs: vec![],
            result_streams: vec![],
            body: SrplProcedureBodyIr {
                operations: vec![
                    SrplBusinessOperationIr {
                        ordinal: 0,
                        kind: SrplBusinessOperationKindIr::Read {
                            source: qn("db.ns.T"),
                            binding: "T".into(),
                            cardinality: Cardinality::Many,
                            predicates: vec![],
                        },
                    },
                    SrplBusinessOperationIr {
                        ordinal: 1,
                        kind: SrplBusinessOperationKindIr::Update {
                            target: qn("db.ns.T"),
                            predicates: vec![],
                            assignments: vec![SrplAssignmentIr {
                                field: "qty".into(),
                                value: SrplValueIr::Bool(false),
                            }],
                            affected_rows_exact: Some(1),
                        },
                    },
                ],
            },
        };
        let cost = estimate_without_stats(&ir);
        assert!(cost.is_valid(), "cost must be valid: {:?}", cost);
        assert!(cost.total_cost > 0.0);
    }
}
