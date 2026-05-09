use super::*;

/// T-IT-01  Full pipeline: normalize → pushdown → liveness → proj_apply
///          on a representative ReserveStock-like body.
#[test]
fn t_it_01_full_optimizer_pipeline_reserve_stock_like() {
    // Simulate: Read Orders → Assert status → Update Orders qty → Emit result.
    let ir = make_ir(vec![
        SrplBusinessOperationIr {
            ordinal: 0,
            kind: SrplBusinessOperationKindIr::Read {
                source: qn("db.Inventory.ProductStock"),
                binding: "Stock".into(),
                cardinality: Cardinality::One,
                predicates: vec![],
            },
        },
        assert_op(1, eq_pred("ProductId", "Stock", "ProductId")),
        update_op(
            2,
            "db.Inventory.ProductStock",
            vec![eq_pred("ProductId", "Stock", "ProductId")],
            vec![SrplAssignmentIr {
                field: "AvailableQuantity".into(),
                value: SrplValueIr::SubtractInput {
                    binding: "Stock".into(),
                    field: "AvailableQuantity".into(),
                    input: "Quantity".into(),
                },
            }],
        ),
        SrplBusinessOperationIr {
            ordinal: 3,
            kind: SrplBusinessOperationKindIr::Emit {
                stream: "Reservation".into(),
                values: vec![SrplEmitValueIr {
                    column: "Reserved".into(),
                    value: bool_const(true),
                }],
            },
        },
    ]);

    // Pass 1: normalize
    let normalized = normalize(ir).unwrap();
    assert!(normalized.body.validate_bounded().is_ok());

    // Pass 2: predicate pushdown
    let pushed = pushdown_apply(normalized).unwrap();
    assert!(pushed.body.validate_bounded().is_ok());

    // Pass 3: liveness
    let lv = ColumnLiveness::compute(&pushed);
    // AvailableQuantity and ProductId are both live after ordinal 0 (used downstream).
    assert!(
        lv.is_live_after(0, "Stock", "AvailableQuantity"),
        "AvailableQuantity must be live"
    );

    // Pass 4: projection pushdown
    let proj_result = proj_apply(pushed.clone());
    assert!(
        !proj_result.projections.is_empty(),
        "must have at least one projection"
    );

    // Pass 5: cost estimate
    let cost_est = estimate_without_stats(&pushed);
    assert!(
        cost_est.is_valid(),
        "cost must be valid after full pipeline"
    );
    assert!(
        cost_est.total_cost > 0.0,
        "non-empty body must have positive cost"
    );
}

/// T-IT-02  Pipeline output plan kind is classified correctly after normalize+pushdown.
#[test]
fn t_it_02_plan_kind_after_full_pipeline() {
    let ir = make_ir(vec![
        read_op(0, "T", vec![]),
        assert_op(1, eq_pred("id", "T", "id")),
    ]);
    let normalized = normalize(ir).unwrap();
    let pushed = pushdown_apply(normalized).unwrap();
    let kind = OptimizerPlanKind::classify(&pushed);
    // After push, Read has an equality predicate → PointLookup.
    assert_eq!(kind, OptimizerPlanKind::PointLookup);
}

/// T-IT-03  Applying the constant fold helper on fold_assignments with no items
///          is a safe no-op.
#[test]
fn t_it_03_fold_assignments_empty_is_noop() {
    let result = fold_assignments(vec![]);
    assert!(
        result.is_empty(),
        "fold_assignments on empty input must return empty"
    );
}

/// T-IT-04  Applying the constant fold helper on fold_emit_values with no items
///          is a safe no-op.
#[test]
fn t_it_04_fold_emit_values_empty_is_noop() {
    let result = fold_emit_values(vec![]);
    assert!(result.is_empty());
}

/// T-IT-05  proj_apply on a many-op body produces as many projections as Reads.
#[test]
fn t_it_05_projection_count_equals_read_count() {
    let ir = make_ir(vec![
        read_op(0, "A", vec![]),
        read_op(1, "B", vec![]),
        read_op(2, "C", vec![]),
        raise_op(3),
    ]);
    let result = proj_apply(ir);
    assert_eq!(
        result.projections.len(),
        3,
        "three reads → three projections"
    );
}

/// T-IT-06  After a full pipeline run, the IR body satisfies validate_bounded.
#[test]
fn t_it_06_post_pipeline_ir_validates_bounded() {
    let ir = make_ir(vec![
        read_op(0, "T", vec![]),
        assert_op(1, eq_pred("id", "T", "id")),
        emit_op(2, "T", "id"),
    ]);
    let normalized = normalize(ir).unwrap();
    let pushed = pushdown_apply(normalized).unwrap();
    assert!(
        pushed.body.validate_bounded().is_ok(),
        "post-pipeline body must pass validate_bounded"
    );
}

/// T-BM-01  fold_value completes 10 000 constant-fold iterations in bounded time.
///
/// This is a soft latency gate: the test passes as long as the optimizer is
/// not quadratic on simple expressions. It does not assert wall-clock time
/// directly; it asserts the loop completes, which would fail under a
/// debugger / profiler timeout if catastrophically slow.
#[test]
fn t_bm_01_constant_fold_10k_iterations() {
    for i in 0..10_000_i64 {
        let expr = arith(ArithOp::Add, int(i), int(i + 1));
        let result = fold_value(expr).unwrap();
        assert_eq!(result, int(i * 2 + 1));
    }
}

/// T-BM-02  simplify_predicates on a list of 16 unique predicates completes
///          in a single pass.
#[test]
fn t_bm_02_simplify_16_unique_predicates() {
    let preds: Vec<SrplPredicateIr> = (0..16u32)
        .map(|i| eq_pred(&format!("p{}", i), "T", &format!("f{}", i)))
        .collect();
    let result = simplify_predicates(preds.clone());
    match result {
        SimplifiedPredicates::Predicates(v) => assert_eq!(v.len(), 16),
        SimplifiedPredicates::AlwaysFalse => panic!("unexpected AlwaysFalse"),
    }
}

/// T-BM-03  estimate_without_stats on a maximum-size body finishes without error.
#[test]
fn t_bm_03_cost_estimate_max_body_size() {
    let ops: Vec<SrplBusinessOperationIr> = (0..MAX_SRPL_BODY_OPERATIONS as u32)
        .map(|i| read_op(i, "T", vec![]))
        .collect();
    let ir = make_ir(ops);
    let cost_est = estimate_without_stats(&ir);
    assert!(
        cost_est.is_valid(),
        "cost must be valid for max-body-size IR"
    );
    assert!(cost_est.total_cost > 0.0);
}

/// T-BM-04  normalize on a maximum-size body with all-duplicate predicates
///          runs and produces a valid, bounded result.
#[test]
fn t_bm_04_normalize_max_body_with_duplicates() {
    let dup_pred = eq_pred("id", "T", "id");
    let ops: Vec<SrplBusinessOperationIr> = (0..MAX_SRPL_BODY_OPERATIONS as u32)
        .map(|i| {
            read_op(
                i,
                &format!("T{}", i),
                vec![dup_pred.clone(), dup_pred.clone()],
            )
        })
        .collect();
    let ir = make_ir(ops);
    let result = normalize(ir).unwrap();
    assert!(result.body.validate_bounded().is_ok());
    // Each Read must have exactly 1 predicate after deduplication.
    for op in &result.body.operations {
        if let SrplBusinessOperationKindIr::Read { predicates, .. } = &op.kind {
            assert_eq!(predicates.len(), 1, "duplicate predicate must be removed");
        }
    }
}

/// T-BM-05  proj_apply on a max-size all-read body does not exceed O(n²) in
///          any observable sense — completes without timeout.
#[test]
fn t_bm_05_proj_apply_max_body() {
    let ops: Vec<SrplBusinessOperationIr> = (0..MAX_SRPL_BODY_OPERATIONS as u32)
        .map(|i| read_op(i, &format!("B{}", i), vec![]))
        .collect();
    let ir = make_ir(ops);
    let result = proj_apply(ir);
    assert_eq!(result.projections.len(), MAX_SRPL_BODY_OPERATIONS);
    // Every projection should have an empty live-column list (no downstream uses).
    for proj in &result.projections {
        let live = proj.live_columns.as_ref().unwrap();
        assert!(live.is_empty(), "no downstream use → empty projection");
    }
}
