/// T-LV-01  Column used only in Emit is live after its Read.
#[test]
fn t_lv_01_emitted_column_live_after_read() {
    let ir = make_ir(vec![read_op(0, "T", vec![]), emit_op(1, "T", "id")]);
    let lv = ColumnLiveness::compute(&ir);
    assert!(lv.is_live_after(0, "T", "id"), "id must be live after Read");
}

/// T-LV-02  Column not emitted or used elsewhere is dead after its Read.
#[test]
fn t_lv_02_unused_column_dead_after_read() {
    let ir = make_ir(vec![read_op(0, "T", vec![]), emit_op(1, "T", "id")]);
    let lv = ColumnLiveness::compute(&ir);
    assert!(
        !lv.is_live_after(0, "T", "name"),
        "name is never used → dead"
    );
    assert!(!lv.is_live_after(0, "T", "qty"), "qty is never used → dead");
}

/// T-LV-03  Column used in Assert predicate is live after its Read.
#[test]
fn t_lv_03_assert_predicate_column_is_live() {
    let ir = make_ir(vec![
        read_op(0, "T", vec![]),
        assert_op(1, eq_pred("s", "T", "status")),
    ]);
    let lv = ColumnLiveness::compute(&ir);
    assert!(lv.is_live_after(0, "T", "status"));
}

/// T-LV-04  Column used in Update assignment is live.
#[test]
fn t_lv_04_update_assignment_field_is_live() {
    let ir = make_ir(vec![
        read_op(0, "T", vec![]),
        update_op(
            1,
            "db.ns.T",
            vec![],
            vec![SrplAssignmentIr {
                field: "qty".into(),
                value: SrplValueIr::Field {
                    binding: "T".into(),
                    field: "qty".into(),
                },
            }],
        ),
    ]);
    let lv = ColumnLiveness::compute(&ir);
    assert!(
        lv.is_live_after(0, "T", "qty"),
        "qty referenced in Update assignment must be live"
    );
}

/// T-LV-05  No operations → liveness returns empty for any query.
#[test]
fn t_lv_05_empty_body_no_live_columns() {
    let ir = empty_ir();
    let lv = ColumnLiveness::compute(&ir);
    assert!(!lv.is_live_after(0, "T", "id"));
    assert!(lv.live_columns_after(0).is_empty());
}

/// T-LV-06  Column is live when used by multiple downstream operations.
#[test]
fn t_lv_06_column_live_across_multiple_uses() {
    let ir = make_ir(vec![
        read_op(0, "T", vec![]),
        assert_op(1, eq_pred("s", "T", "status")),
        emit_op(2, "T", "status"),
    ]);
    let lv = ColumnLiveness::compute(&ir);
    assert!(
        lv.is_live_after(0, "T", "status"),
        "used twice downstream → live"
    );
}

/// T-LV-07  live_columns_after returns correct set for a multi-column emit.
#[test]
fn t_lv_07_live_columns_after_multi_emit() {
    let multi_emit = SrplBusinessOperationIr {
        ordinal: 1,
        kind: SrplBusinessOperationKindIr::Emit {
            stream: "S".into(),
            values: vec![
                SrplEmitValueIr {
                    column: "out_id".into(),
                    value: SrplValueIr::Field {
                        binding: "T".into(),
                        field: "id".into(),
                    },
                },
                SrplEmitValueIr {
                    column: "out_name".into(),
                    value: SrplValueIr::Field {
                        binding: "T".into(),
                        field: "name".into(),
                    },
                },
            ],
        },
    };
    let ir = make_ir(vec![read_op(0, "T", vec![]), multi_emit]);
    let lv = ColumnLiveness::compute(&ir);
    let live = lv.live_columns_after(0);
    let fields: Vec<&str> = live.iter().map(|(_, f)| f.as_str()).collect();
    assert!(fields.contains(&"id"), "id must be live");
    assert!(fields.contains(&"name"), "name must be live");
}

/// T-LV-08  Second binding does not interfere with first binding's liveness.
#[test]
fn t_lv_08_two_bindings_independent_liveness() {
    let ir = make_ir(vec![
        read_op(0, "A", vec![]),
        read_op(1, "B", vec![]),
        emit_op(2, "A", "x"),
        emit_op(3, "B", "y"),
    ]);
    let lv = ColumnLiveness::compute(&ir);
    assert!(lv.is_live_after(0, "A", "x"));
    assert!(
        !lv.is_live_after(0, "B", "y"),
        "B not yet defined at ordinal 0"
    );
    assert!(lv.is_live_after(1, "B", "y"));
}

/// T-PJ-01  Only emitted column appears in projection.
#[test]
fn t_pj_01_only_emitted_column_in_projection() {
    let ir = make_ir(vec![read_op(0, "T", vec![]), emit_op(1, "T", "id")]);
    let res = proj_apply(ir);
    assert_eq!(res.projections.len(), 1);
    let live = res.projections[0].live_columns.as_ref().unwrap();
    assert!(live.contains(&"id".to_string()));
    assert!(!live.contains(&"name".to_string()));
}

/// T-PJ-02  Body with no Read → empty projections.
#[test]
fn t_pj_02_no_read_produces_no_projections() {
    let ir = make_ir(vec![raise_op(0)]);
    let res = proj_apply(ir);
    assert!(
        res.projections.is_empty(),
        "Raise-only body has no projections"
    );
}

/// T-PJ-03  INV-12: projection never removes a live column.
#[test]
fn t_pj_03_inv12_live_column_never_removed() {
    let ir = make_ir(vec![
        read_op(0, "T", vec![]),
        assert_op(1, eq_pred("s", "T", "status")),
        emit_op(2, "T", "id"),
    ]);
    let res = proj_apply(ir.clone());
    let lv = ColumnLiveness::compute(&ir);
    let live_at_0 = lv.live_columns_after(0);
    let proj = res.projections[0].live_columns.as_ref().unwrap();
    for (binding, field) in &live_at_0 {
        if binding == "T" {
            assert!(
                proj.contains(field),
                "INV-12 violated: live column '{}' missing from projection",
                field
            );
        }
    }
}

/// T-PJ-04  Multiple reads produce one projection entry each.
#[test]
fn t_pj_04_multiple_reads_one_projection_each() {
    let ir = make_ir(vec![
        read_op(0, "A", vec![]),
        read_op(1, "B", vec![]),
        emit_op(2, "A", "x"),
        emit_op(3, "B", "y"),
    ]);
    let res = proj_apply(ir);
    assert_eq!(res.projections.len(), 2);
}

/// T-PJ-05  No downstream use → projection has empty live column list.
#[test]
fn t_pj_05_no_downstream_use_yields_empty_projection() {
    // Read but nothing uses it.
    let ir = make_ir(vec![read_op(0, "T", vec![])]);
    let res = proj_apply(ir);
    let live = res.projections[0].live_columns.as_ref().unwrap();
    assert!(
        live.is_empty(),
        "No column is used → projection must be empty (count-only)"
    );
}

/// T-PJ-06  proj_apply does NOT mutate the IR body structure (annotation-only).
#[test]
fn t_pj_06_apply_does_not_mutate_ir_body() {
    let ir = make_ir(vec![read_op(0, "T", vec![]), emit_op(1, "T", "id")]);
    let original_ops_len = ir.body.operations.len();
    let res = proj_apply(ir);
    assert_eq!(
        res.ir.body.operations.len(),
        original_ops_len,
        "proj_apply must not add or remove IR operations"
    );
}

/// T-PJ-07  Projection binding name matches the Read binding.
#[test]
fn t_pj_07_projection_binding_matches_read() {
    let ir = make_ir(vec![
        read_op(0, "MyBinding", vec![]),
        emit_op(1, "MyBinding", "col"),
    ]);
    let res = proj_apply(ir);
    assert_eq!(res.projections[0].binding, "MyBinding");
}

/// T-PJ-08  Multiple columns emitted: all appear in projection.
#[test]
fn t_pj_08_multiple_emitted_columns_all_in_projection() {
    let emit = SrplBusinessOperationIr {
        ordinal: 1,
        kind: SrplBusinessOperationKindIr::Emit {
            stream: "S".into(),
            values: vec![
                SrplEmitValueIr {
                    column: "c1".into(),
                    value: SrplValueIr::Field {
                        binding: "T".into(),
                        field: "col1".into(),
                    },
                },
                SrplEmitValueIr {
                    column: "c2".into(),
                    value: SrplValueIr::Field {
                        binding: "T".into(),
                        field: "col2".into(),
                    },
                },
                SrplEmitValueIr {
                    column: "c3".into(),
                    value: SrplValueIr::Field {
                        binding: "T".into(),
                        field: "col3".into(),
                    },
                },
            ],
        },
    };
    let ir = make_ir(vec![read_op(0, "T", vec![]), emit]);
    let res = proj_apply(ir);
    let live = res.projections[0].live_columns.as_ref().unwrap();
    assert!(live.contains(&"col1".to_string()));
    assert!(live.contains(&"col2".to_string()));
    assert!(live.contains(&"col3".to_string()));
}

/// T-PP-01  Assert after Read is retained because it owns the failure code.
#[test]
fn t_pp_01_basic_assert_retained_after_upstream_read() {
    let pred = eq_pred("id", "T", "id");
    let ir = make_ir(vec![read_op(0, "T", vec![]), assert_op(1, pred.clone())]);
    let result = pushdown_apply(ir).unwrap();
    assert_eq!(
        result.body.operations.len(),
        2,
        "Assert must stay until Read can carry its failure contract"
    );
    match &result.body.operations[0].kind {
        SrplBusinessOperationKindIr::Read { predicates, .. } => {
            assert!(predicates.is_empty());
        }
        _ => panic!("expected Read"),
    }
    assert!(matches!(
        &result.body.operations[1].kind,
        SrplBusinessOperationKindIr::Assert { predicate, .. } if predicate == &pred
    ));
}

/// T-PP-02  Assert without upstream Read stays in place.
#[test]
fn t_pp_02_assert_without_upstream_read_stays() {
    let pred = eq_pred("id", "UNKNOWN", "id");
    let ir = make_ir(vec![assert_op(0, pred)]);
    let result = pushdown_apply(ir).unwrap();
    assert_eq!(result.body.operations.len(), 1);
    assert!(matches!(
        result.body.operations[0].kind,
        SrplBusinessOperationKindIr::Assert { .. }
    ));
}

/// T-PP-03  Empty body survives pushdown as a no-op.
#[test]
fn t_pp_03_empty_body_noop() {
    let ir = empty_ir();
    let result = pushdown_apply(ir).unwrap();
    assert!(result.body.operations.is_empty());
}

/// T-PP-04  Ordinals are dense and zero-based after safe no-op pushdown.
#[test]
fn t_pp_04_ordinals_dense_after_safe_noop_pushdown() {
    let pred = eq_pred("id", "T", "id");
    let ir = make_ir(vec![read_op(0, "T", vec![]), assert_op(1, pred)]);
    let result = pushdown_apply(ir).unwrap();
    for (i, op) in result.body.operations.iter().enumerate() {
        assert_eq!(
            op.ordinal, i as u32,
            "ordinal must equal position after pushdown"
        );
    }
}

/// T-PP-05  GTE assert is retained in the range-predicate case.
#[test]
fn t_pp_05_gte_assert_retained() {
    let pred = gte_pred("T", "score", "min_score");
    let ir = make_ir(vec![read_op(0, "T", vec![]), assert_op(1, pred.clone())]);
    let result = pushdown_apply(ir).unwrap();
    assert_eq!(result.body.operations.len(), 2);
    match &result.body.operations[0].kind {
        SrplBusinessOperationKindIr::Read { predicates, .. } => {
            assert!(predicates.is_empty());
        }
        _ => panic!("expected Read"),
    }
    assert!(matches!(
        &result.body.operations[1].kind,
        SrplBusinessOperationKindIr::Assert { predicate, .. } if predicate == &pred
    ));
}

/// T-PP-06  Raise-only body is unaffected by pushdown.
#[test]
fn t_pp_06_raise_only_body_unaffected() {
    let ir = make_ir(vec![raise_op(0)]);
    let result = pushdown_apply(ir).unwrap();
    assert_eq!(result.body.operations.len(), 1);
    assert!(matches!(
        result.body.operations[0].kind,
        SrplBusinessOperationKindIr::Raise { .. }
    ));
}

/// T-PP-07  Assert predicate bound to wrong binding not pushed.
#[test]
fn t_pp_07_wrong_binding_not_pushed() {
    let pred = eq_pred("id", "WRONG", "id");
    let ir = make_ir(vec![read_op(0, "T", vec![]), assert_op(1, pred.clone())]);
    let result = pushdown_apply(ir).unwrap();
    assert_eq!(
        result.body.operations.len(),
        2,
        "Assert must stay; binding mismatch"
    );
}
