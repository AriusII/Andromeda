use andromeda_plan_cache::PlanClass;
use andromeda_srpl_ir::{
    Cardinality, SrplBusinessOperationKindIr, SrplPredicateIr, SrplProcedureIr,
};

/// Internal plan choice taxonomy.
///
/// `OptimizerPlanKind` is transient optimizer state. Cache keys use
/// `PlanClass`; this enum must not be stored in `PlanCacheKey`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizerPlanKind {
    /// Single-row insert via a fully-bound parameter set.
    SingleRowInsert,
    /// Bulk insert via a cardinality-Many input set.
    BulkInsert,
    /// Read by exact primary key equality.
    PointLookup,
    /// Read with a range predicate.
    RangeScan,
    /// Read with aggregation (future extension).
    Aggregation,
    /// Multi-table equi-join (future extension).
    Join,
    /// Windowed aggregation (future extension).
    WindowFunction,
    /// Key→value Map lookup.
    MapLookup,
    /// Union of compatible result streams (future extension).
    UnionAll,
}

impl OptimizerPlanKind {
    /// Classify a `SrplProcedureIr` body into its dominant plan kind.
    ///
    /// Rules applied in priority order (first match wins):
    /// 1. Single read with `Cardinality::One` and an equality predicate → `PointLookup`.
    /// 2. Single read with `Cardinality::One` and a range predicate → `RangeScan`.
    /// 3. Any read with `Cardinality::Many` or `Cardinality::NonEmptyMany` and only
    ///    equality predicates, plus at least one update → `BulkInsert`.
    /// 4. Any read with a range predicate → `RangeScan`.
    /// 5. Default → `SingleRowInsert`.
    pub fn classify(ir: &SrplProcedureIr) -> Self {
        let mut has_range = false;
        let mut has_many = false;
        let mut has_update = false;
        let mut has_read = false;
        let mut all_reads_have_equality = true;

        for (index, op) in ir.body.operations.iter().enumerate() {
            match &op.kind {
                SrplBusinessOperationKindIr::Read {
                    binding,
                    cardinality,
                    predicates,
                    ..
                } => {
                    has_read = true;
                    match cardinality {
                        Cardinality::Many | Cardinality::NonEmptyMany => has_many = true,
                        _ => {},
                    }
                    let assert_predicates =
                        downstream_assert_predicates_for_binding(ir, index, binding);
                    for pred in predicates.iter().chain(assert_predicates) {
                        if matches!(pred, SrplPredicateIr::FieldGreaterThanOrEqualInput { .. }) {
                            has_range = true;
                        }
                    }
                    if predicates.is_empty()
                        && !has_downstream_assert_predicate_for_binding(ir, index, binding)
                    {
                        all_reads_have_equality = false;
                    }
                },
                SrplBusinessOperationKindIr::Update { .. } => {
                    has_update = true;
                },
                _ => {},
            }
        }

        if has_read && !has_many && !has_range && all_reads_have_equality {
            return Self::PointLookup;
        }
        if has_range {
            return Self::RangeScan;
        }
        if has_many && has_update {
            return Self::BulkInsert;
        }
        if has_many {
            return Self::BulkInsert;
        }
        Self::SingleRowInsert
    }

    /// Map to the coarse external class accepted by plan-cache keys.
    pub const fn to_plan_class(self) -> PlanClass {
        match self {
            Self::SingleRowInsert | Self::PointLookup | Self::MapLookup => PlanClass::Singleton,

            Self::BulkInsert | Self::RangeScan | Self::UnionAll => PlanClass::Cardinality,

            Self::Aggregation | Self::Join | Self::WindowFunction => PlanClass::StatsAdaptive,
        }
    }

    /// Lower priority (smaller number) = simpler = preferred on cost ties.
    pub const fn priority(self) -> u8 {
        match self {
            Self::PointLookup => 0,
            Self::SingleRowInsert => 1,
            Self::MapLookup => 2,
            Self::BulkInsert => 3,
            Self::RangeScan => 4,
            Self::UnionAll => 5,
            Self::Aggregation => 6,
            Self::Join => 7,
            Self::WindowFunction => 8,
        }
    }
}

fn downstream_assert_predicates_for_binding<'a>(
    ir: &'a SrplProcedureIr,
    read_index: usize,
    binding: &'a str,
) -> impl Iterator<Item = &'a SrplPredicateIr> {
    ir.body.operations[read_index + 1..]
        .iter()
        .take_while(|operation| {
            !matches!(
                &operation.kind,
                SrplBusinessOperationKindIr::Update { .. }
                    | SrplBusinessOperationKindIr::Emit { .. }
                    | SrplBusinessOperationKindIr::Raise { .. }
            )
        })
        .filter_map(|operation| match &operation.kind {
            SrplBusinessOperationKindIr::Assert { predicate, .. }
                if predicate_references_binding(predicate, binding) =>
            {
                Some(predicate)
            },
            _ => None,
        })
}

fn has_downstream_assert_predicate_for_binding(
    ir: &SrplProcedureIr,
    read_index: usize,
    binding: &str,
) -> bool {
    downstream_assert_predicates_for_binding(ir, read_index, binding)
        .next()
        .is_some()
}

fn predicate_references_binding(predicate: &SrplPredicateIr, binding: &str) -> bool {
    match predicate {
        SrplPredicateIr::InputEqualsField {
            binding: predicate_binding,
            ..
        }
        | SrplPredicateIr::FieldGreaterThanOrEqualInput {
            binding: predicate_binding,
            ..
        } => predicate_binding == binding,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_plan_cache::PlanClass;

    #[test]
    fn singleton_plan_kinds_map_to_singleton_class() {
        assert_eq!(
            OptimizerPlanKind::PointLookup.to_plan_class(),
            PlanClass::Singleton
        );
        assert_eq!(
            OptimizerPlanKind::SingleRowInsert.to_plan_class(),
            PlanClass::Singleton
        );
        assert_eq!(
            OptimizerPlanKind::MapLookup.to_plan_class(),
            PlanClass::Singleton
        );
    }

    #[test]
    fn bulk_maps_to_cardinality_class() {
        assert_eq!(
            OptimizerPlanKind::BulkInsert.to_plan_class(),
            PlanClass::Cardinality
        );
        assert_eq!(
            OptimizerPlanKind::RangeScan.to_plan_class(),
            PlanClass::Cardinality
        );
    }

    #[test]
    fn complex_maps_to_stats_adaptive() {
        assert_eq!(
            OptimizerPlanKind::Join.to_plan_class(),
            PlanClass::StatsAdaptive
        );
        assert_eq!(
            OptimizerPlanKind::Aggregation.to_plan_class(),
            PlanClass::StatsAdaptive
        );
    }

    #[test]
    fn priority_point_lookup_is_zero() {
        assert_eq!(OptimizerPlanKind::PointLookup.priority(), 0);
    }
}
