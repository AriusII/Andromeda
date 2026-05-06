/// T-CM-01  Empty body → zero cost.
#[test]
fn t_cm_01_empty_body_zero_cost() {
    let cost_est = estimate_without_stats(&empty_ir());
    assert!(cost_est.is_valid());
    assert_eq!(cost_est.total_cost, 0.0);
}

/// T-CM-02  Single Read Cardinality::One → cost ≥ 0 and valid.
#[test]
fn t_cm_02_single_read_one_valid_nonnegative() {
    let ir = make_ir(vec![read_op(0, "T", vec![])]);
    let cost_est = estimate_without_stats(&ir);
    assert!(cost_est.is_valid());
    assert!(cost_est.total_cost >= 0.0);
}

/// T-CM-03  Single Read Cardinality::Many → higher cost than Cardinality::One.
#[test]
fn t_cm_03_cardinality_many_higher_cost_than_one() {
    let ir_one = make_ir(vec![read_op(0, "T", vec![])]);
    let ir_many = make_ir(vec![read_op_many(0, "T")]);
    let cost_one = estimate_without_stats(&ir_one);
    let cost_many = estimate_without_stats(&ir_many);
    assert!(
        cost_many.total_cost > cost_one.total_cost,
        "Many-cardinality read must have higher cost than One"
    );
}

/// T-CM-04  Cost estimate is non-negative for every operation mix.
#[test]
fn t_cm_04_cost_always_nonnegative() {
    let ir = make_ir(vec![
        read_op(0, "T", vec![]),
        update_op(1, "db.ns.T", vec![], vec![simple_assignment("x", int(1))]),
        raise_op(2),
    ]);
    let cost_est = estimate_without_stats(&ir);
    assert!(cost_est.cpu_cost >= 0.0, "cpu_cost non-negative");
    assert!(cost_est.io_cost >= 0.0, "io_cost non-negative");
    assert!(cost_est.memory_cost >= 0.0, "memory_cost non-negative");
    assert!(cost_est.total_cost >= 0.0, "total_cost non-negative");
}

/// T-CM-05  CostEstimate::zero() is valid.
#[test]
fn t_cm_05_zero_estimate_is_valid() {
    assert!(CostEstimate::zero().is_valid());
    assert_eq!(CostEstimate::zero().total_cost, 0.0);
}

/// T-CM-06  cost_accuracy returns 0.0 when estimated cost is zero.
#[test]
fn t_cm_06_accuracy_zero_estimate_returns_zero() {
    let actual = ActualCost {
        cpu_nanos: 1_000,
        io_pages: 1,
        memory_pages: 1,
    };
    assert_eq!(cost_accuracy(&CostEstimate::zero(), &actual), 0.0);
}

/// T-CM-07  cost_accuracy ratio exceeds WARN_THRESHOLD when actual >> estimated.
#[test]
fn t_cm_07_high_actual_exceeds_warn_threshold() {
    let ir = make_ir(vec![read_op(0, "T", vec![])]);
    let estimated = estimate_without_stats(&ir);
    let actual = ActualCost {
        cpu_nanos: 0,
        io_pages: 1_000,
        memory_pages: 0,
    };
    let ratio = cost_accuracy(&estimated, &actual);
    assert!(
        ratio > COST_ACCURACY_WARN_THRESHOLD,
        "expected ratio > {}, got {}",
        COST_ACCURACY_WARN_THRESHOLD,
        ratio
    );
}

/// T-CM-08  cost_accuracy ratio exceeds ALERT_THRESHOLD for extreme divergence.
#[test]
fn t_cm_08_extreme_actual_exceeds_alert_threshold() {
    let ir = make_ir(vec![read_op(0, "T", vec![])]);
    let estimated = estimate_without_stats(&ir);
    let actual = ActualCost {
        cpu_nanos: 0,
        io_pages: 100_000,
        memory_pages: 0,
    };
    let ratio = cost_accuracy(&estimated, &actual);
    assert!(
        ratio > COST_ACCURACY_ALERT_THRESHOLD,
        "expected ratio > {}, got {}",
        COST_ACCURACY_ALERT_THRESHOLD,
        ratio
    );
}

/// T-CM-09  Adding predicates to a Read reduces estimated tuple count (selectivity).
#[test]
fn t_cm_09_predicates_reduce_cost_via_selectivity() {
    let ir_no_pred = make_ir(vec![read_op_many(0, "T")]);
    let ir_with_pred = make_ir(vec![SrplBusinessOperationIr {
        ordinal: 0,
        kind: SrplBusinessOperationKindIr::Read {
            source: qn("db.ns.T"),
            binding: "T".into(),
            cardinality: Cardinality::Many,
            predicates: vec![eq_pred("id", "T", "id")],
        },
    }]);
    let cost_no = estimate_without_stats(&ir_no_pred);
    let cost_with = estimate_without_stats(&ir_with_pred);
    assert!(
        cost_with.total_cost < cost_no.total_cost,
        "Predicates must reduce estimated cost (selectivity)"
    );
}

/// T-CM-10  cost_accuracy is non-negative.
#[test]
fn t_cm_10_accuracy_ratio_nonnegative() {
    let ir = make_ir(vec![read_op(0, "T", vec![])]);
    let estimated = estimate_without_stats(&ir);
    let actual = ActualCost {
        cpu_nanos: 500,
        io_pages: 2,
        memory_pages: 1,
    };
    let ratio = cost_accuracy(&estimated, &actual);
    assert!(ratio >= 0.0, "accuracy ratio must never be negative");
}

/// T-CM-11  total_cost must be NaN-free and finite.
#[test]
fn t_cm_11_cost_is_finite_and_not_nan() {
    let ir = make_ir(vec![read_op_many(0, "T"), read_op(1, "U", vec![])]);
    let cost_est = estimate_without_stats(&ir);
    assert!(!cost_est.total_cost.is_nan(), "total_cost must not be NaN");
    assert!(
        !cost_est.total_cost.is_infinite(),
        "total_cost must not be infinite"
    );
}

/// T-PK-01  Body with single Read::One + equality predicate → PointLookup.
#[test]
fn t_pk_01_single_read_equality_is_point_lookup() {
    let ir = make_ir(vec![read_op(0, "T", vec![eq_pred("id", "T", "id")])]);
    assert_eq!(
        OptimizerPlanKind::classify(&ir),
        OptimizerPlanKind::PointLookup
    );
}

/// T-PK-02  Body with Many cardinality → BulkInsert (no range predicate).
#[test]
fn t_pk_02_many_cardinality_is_bulk_insert() {
    let ir = make_ir(vec![read_op_many(0, "T")]);
    assert_eq!(
        OptimizerPlanKind::classify(&ir),
        OptimizerPlanKind::BulkInsert
    );
}

/// T-PK-03  Body with GTE predicate → RangeScan.
#[test]
fn t_pk_03_gte_predicate_is_range_scan() {
    let ir = make_ir(vec![SrplBusinessOperationIr {
        ordinal: 0,
        kind: SrplBusinessOperationKindIr::Read {
            source: qn("db.ns.T"),
            binding: "T".into(),
            cardinality: Cardinality::One,
            predicates: vec![gte_pred("T", "score", "min")],
        },
    }]);
    assert_eq!(
        OptimizerPlanKind::classify(&ir),
        OptimizerPlanKind::RangeScan
    );
}

/// T-PK-04  Empty body defaults to SingleRowInsert.
#[test]
fn t_pk_04_empty_body_is_single_row_insert() {
    assert_eq!(
        OptimizerPlanKind::classify(&empty_ir()),
        OptimizerPlanKind::SingleRowInsert
    );
}

/// T-PK-05  PointLookup maps to PlanClass::Singleton.
#[test]
fn t_pk_05_point_lookup_maps_to_singleton() {
    assert_eq!(
        OptimizerPlanKind::PointLookup.to_plan_class(),
        PlanClass::Singleton
    );
}

/// T-PK-06  RangeScan maps to PlanClass::Cardinality.
#[test]
fn t_pk_06_range_scan_maps_to_cardinality() {
    assert_eq!(
        OptimizerPlanKind::RangeScan.to_plan_class(),
        PlanClass::Cardinality
    );
}

/// T-PK-07  Join maps to PlanClass::StatsAdaptive.
#[test]
fn t_pk_07_join_maps_to_stats_adaptive() {
    assert_eq!(
        OptimizerPlanKind::Join.to_plan_class(),
        PlanClass::StatsAdaptive
    );
}

/// T-PK-08  PointLookup has priority 0 (preferred on cost tie).
#[test]
fn t_pk_08_point_lookup_priority_zero() {
    assert_eq!(OptimizerPlanKind::PointLookup.priority(), 0);
}

/// T-PC-01  choose selects the minimum cost plan.
#[test]
fn t_pc_01_choose_minimum_cost_plan() {
    let alts = vec![
        (empty_ir(), cost(100.0), OptimizerPlanKind::RangeScan),
        (empty_ir(), cost(20.0), OptimizerPlanKind::PointLookup),
        (empty_ir(), cost(50.0), OptimizerPlanKind::SingleRowInsert),
    ];
    let result = choose(alts).unwrap();
    assert_eq!(result.chosen_kind, OptimizerPlanKind::PointLookup);
    assert!((result.chosen_cost.total_cost - 20.0).abs() < 0.001);
}

/// T-PC-02  Tie broken by priority (lower ordinal wins).
#[test]
fn t_pc_02_tie_broken_by_priority() {
    let alts = vec![
        (empty_ir(), cost(10.0), OptimizerPlanKind::RangeScan),
        (empty_ir(), cost(10.0), OptimizerPlanKind::PointLookup),
    ];
    let result = choose(alts).unwrap();
    assert_eq!(result.chosen_kind, OptimizerPlanKind::PointLookup);
}

/// T-PC-03  Single alternative is always chosen.
#[test]
fn t_pc_03_single_alternative_always_chosen() {
    let alts = vec![(empty_ir(), cost(42.0), OptimizerPlanKind::MapLookup)];
    let result = choose(alts).unwrap();
    assert_eq!(result.chosen_kind, OptimizerPlanKind::MapLookup);
    assert!(result.all_alternatives[0].chosen);
}

/// T-PC-04  Empty alternatives → error (not panic).
#[test]
fn t_pc_04_empty_alternatives_returns_error() {
    let result = choose(vec![]);
    assert!(
        result.is_err(),
        "choose with zero alternatives must return an error"
    );
}

/// T-PC-05  Rejected alternative receives RejectionReason.
#[test]
fn t_pc_05_rejected_alternatives_have_rejection_reason() {
    let alts = vec![
        (empty_ir(), cost(50.0), OptimizerPlanKind::RangeScan),
        (empty_ir(), cost(10.0), OptimizerPlanKind::PointLookup),
    ];
    let result = choose(alts).unwrap();
    let rejected = result.all_alternatives.iter().find(|r| !r.chosen).unwrap();
    assert!(
        rejected.rejection.is_some(),
        "rejected alternative must have a reason"
    );
    assert_eq!(rejected.rejection, Some(RejectionReason::HigherCost));
}

/// T-FK-01  DATE_ADD is pure deterministic → foldable.
#[test]
fn t_fk_01_date_add_is_foldable() {
    let det = classify_builtin("DATE_ADD");
    assert_eq!(det, FunctionDeterminism::PureDeterministic);
    assert!(det.is_foldable());
}

/// T-FK-02  NOW is non-deterministic per invocation → NOT foldable.
#[test]
fn t_fk_02_now_not_foldable() {
    let det = classify_builtin("NOW");
    assert_eq!(det, FunctionDeterminism::NonDeterministicPerInvocation);
    assert!(!det.is_foldable());
}

/// T-FK-03  RAND is non-deterministic per invocation.
#[test]
fn t_fk_03_rand_nondeterministic_per_invocation() {
    assert_eq!(
        classify_builtin("RAND"),
        FunctionDeterminism::NonDeterministicPerInvocation
    );
}

/// T-FK-04  TRANSACTION_TIMESTAMP is non-deterministic per transaction.
#[test]
fn t_fk_04_transaction_timestamp_per_transaction() {
    assert_eq!(
        classify_builtin("TRANSACTION_TIMESTAMP"),
        FunctionDeterminism::NonDeterministicPerTransaction
    );
}

/// T-FK-05  Unknown function defaults to NonDeterministicPerInvocation (safe pessimist).
#[test]
fn t_fk_05_unknown_function_defaults_to_nondeterministic() {
    let det = classify_builtin("MY_CUSTOM_MAGIC_FN");
    assert_eq!(det, FunctionDeterminism::NonDeterministicPerInvocation);
    assert!(
        !det.is_foldable(),
        "unknown function must never be foldable"
    );
}

/// T-FK-06  ABS is pure deterministic.
#[test]
fn t_fk_06_abs_is_pure_deterministic() {
    assert_eq!(
        classify_builtin("ABS"),
        FunctionDeterminism::PureDeterministic
    );
}

/// T-FK-07  UUID is non-deterministic per invocation.
#[test]
fn t_fk_07_uuid_nondeterministic() {
    assert_eq!(
        classify_builtin("UUID"),
        FunctionDeterminism::NonDeterministicPerInvocation
    );
}

/// T-FK-08  All pure-deterministic functions must be marked foldable.
#[test]
fn t_fk_08_all_listed_pure_deterministic_are_foldable() {
    let pure_fns = [
        "DATE_ADD",
        "DATE_SUB",
        "ABS",
        "COALESCE",
        "LENGTH",
        "UPPER",
        "LOWER",
        "TRIM",
        "LTRIM",
        "RTRIM",
        "CHAR_LENGTH",
        "CONCAT",
        "REPLACE",
        "SUBSTRING",
        "MOD",
        "POWER",
        "FLOOR",
        "CEILING",
        "ROUND",
        "SIGN",
        "GREATEST",
        "LEAST",
    ];
    for name in pure_fns {
        let det = classify_builtin(name);
        assert_eq!(
            det,
            FunctionDeterminism::PureDeterministic,
            "{} must be PureDeterministic",
            name
        );
        assert!(det.is_foldable(), "{} must be foldable", name);
    }
}

/// T-FK-09  Non-deterministic functions must NOT be marked foldable.
#[test]
fn t_fk_09_nondeterministic_functions_never_foldable() {
    let nondets = [
        "NOW",
        "RAND",
        "RANDOM",
        "UUID",
        "NEWID",
        "GEN_RANDOM_UUID",
        "CURRENT_TIMESTAMP",
        "SYSDATE",
        "GETDATE",
        "GETUTCDATE",
        "SYSDATETIME",
        "TRANSACTION_TIMESTAMP",
        "LOCALTIMESTAMP",
        "CURRENT_DATE",
        "CURRENT_TIME",
    ];
    for name in nondets {
        let det = classify_builtin(name);
        assert!(!det.is_foldable(), "{} must NOT be foldable (INV-08)", name);
    }
}

/// T-PH-01  PHASE_COUNT is 8 (stable doctrine count).
#[test]
fn t_ph_01_phase_count_is_eight() {
    assert_eq!(OptimizerPhase::PHASE_COUNT, 8);
}

/// T-PH-02  All phases are strictly ordered by their ordinals.
#[test]
fn t_ph_02_phases_are_strictly_ordered() {
    let all = [
        OptimizerPhase::Parsing,
        OptimizerPhase::Binding,
        OptimizerPhase::IRLowering,
        OptimizerPhase::ConstantFolding,
        OptimizerPhase::PredicatePushdown,
        OptimizerPhase::ProjectionPushdown,
        OptimizerPhase::CostAnalysis,
        OptimizerPhase::PlanChoice,
    ];
    for window in all.windows(2) {
        assert!(
            window[0].as_ordinal() < window[1].as_ordinal(),
            "{:?} must precede {:?}",
            window[0],
            window[1]
        );
    }
}

/// T-PH-03  requires_phase reflects immediate predecessor only.
#[test]
fn t_ph_03_requires_phase_immediate_predecessor_only() {
    assert!(OptimizerPhase::ConstantFolding.requires_phase(OptimizerPhase::IRLowering));
    assert!(!OptimizerPhase::ConstantFolding.requires_phase(OptimizerPhase::Binding));
    assert!(!OptimizerPhase::ConstantFolding.requires_phase(OptimizerPhase::PredicatePushdown));
}

/// T-PH-04  PlanChoice requires CostAnalysis as its predecessor.
#[test]
fn t_ph_04_plan_choice_requires_cost_analysis() {
    assert!(OptimizerPhase::PlanChoice.requires_phase(OptimizerPhase::CostAnalysis));
    assert!(!OptimizerPhase::PlanChoice.requires_phase(OptimizerPhase::IRLowering));
}
