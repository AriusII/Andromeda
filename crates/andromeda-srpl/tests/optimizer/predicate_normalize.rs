use super::*;

/// T-PF-01  Empty predicate list → Predicates([])
#[test]
fn t_pf_01_empty_list_yields_empty_predicates() {
    assert_eq!(
        simplify_predicates(vec![]),
        SimplifiedPredicates::Predicates(vec![])
    );
}

/// T-PF-02  Single predicate preserved unchanged.
#[test]
fn t_pf_02_single_predicate_preserved() {
    let p = eq_pred("id", "T", "id");
    let result = simplify_predicates(vec![p.clone()]);
    assert_eq!(result, SimplifiedPredicates::Predicates(vec![p]));
}

/// T-PF-03  Exact duplicate InputEqualsField → deduplicated to one.
#[test]
fn t_pf_03_exact_duplicate_removed() {
    let p = eq_pred("id", "T", "id");
    let result = simplify_predicates(vec![p.clone(), p.clone()]);
    assert_eq!(result, SimplifiedPredicates::Predicates(vec![p]));
}

/// T-PF-04  Triplicate InputEqualsField → one entry.
#[test]
fn t_pf_04_triplicate_deduplicated() {
    let p = eq_pred("x", "R", "x");
    let result = simplify_predicates(vec![p.clone(), p.clone(), p.clone()]);
    assert_eq!(result, SimplifiedPredicates::Predicates(vec![p]));
}

/// T-PF-05  Two distinct predicates both preserved.
#[test]
fn t_pf_05_distinct_predicates_both_preserved() {
    let p1 = eq_pred("id", "T", "id");
    let p2 = eq_pred("name", "T", "name");
    let result = simplify_predicates(vec![p1.clone(), p2.clone()]);
    assert_eq!(result, SimplifiedPredicates::Predicates(vec![p1, p2]));
}

/// T-PF-06  Three distinct predicates all preserved.
#[test]
fn t_pf_06_three_distinct_predicates_all_preserved() {
    let p1 = eq_pred("a", "T", "a");
    let p2 = eq_pred("b", "T", "b");
    let p3 = eq_pred("c", "T", "c");
    let result = simplify_predicates(vec![p1.clone(), p2.clone(), p3.clone()]);
    assert_eq!(result, SimplifiedPredicates::Predicates(vec![p1, p2, p3]));
}

/// T-PF-07  Duplicate GTE predicate → deduplicated.
#[test]
fn t_pf_07_gte_duplicate_removed() {
    let p = gte_pred("T", "created_at", "since");
    let result = simplify_predicates(vec![p.clone(), p.clone()]);
    assert_eq!(result, SimplifiedPredicates::Predicates(vec![p]));
}

/// T-PF-08  Mixed EQ + GTE, both unique → both preserved.
#[test]
fn t_pf_08_mixed_eq_gte_unique_both_preserved() {
    let eq = eq_pred("id", "T", "id");
    let gte = gte_pred("T", "created_at", "since");
    let result = simplify_predicates(vec![eq.clone(), gte.clone()]);
    assert_eq!(result, SimplifiedPredicates::Predicates(vec![eq, gte]));
}

/// T-PF-09  Mixed EQ + GTE with duplicate EQ → EQ deduplicated, GTE kept.
#[test]
fn t_pf_09_mixed_with_duplicate_eq_deduped() {
    let eq = eq_pred("id", "T", "id");
    let gte = gte_pred("T", "rank", "min_rank");
    let result = simplify_predicates(vec![eq.clone(), gte.clone(), eq.clone()]);
    assert_eq!(result, SimplifiedPredicates::Predicates(vec![eq, gte]));
}

/// T-PF-10  Output length ≤ input length (never grows).
#[test]
fn t_pf_10_output_never_grows() {
    let p1 = eq_pred("a", "T", "a");
    let p2 = eq_pred("b", "T", "b");
    let input = vec![p1.clone(), p2.clone(), p1.clone()];
    let result = match simplify_predicates(input.clone()) {
        SimplifiedPredicates::Predicates(v) => v,
        SimplifiedPredicates::AlwaysFalse => vec![],
    };
    assert!(
        result.len() <= input.len(),
        "predicate list must never grow"
    );
}

/// T-PF-11  Idempotency: simplify(simplify(list)) == simplify(list).
#[test]
fn t_pf_11_simplify_is_idempotent() {
    let p = eq_pred("id", "T", "id");
    let input = vec![p.clone(), p.clone()];
    let once = match simplify_predicates(input) {
        SimplifiedPredicates::Predicates(v) => v,
        SimplifiedPredicates::AlwaysFalse => vec![],
    };
    let twice = match simplify_predicates(once.clone()) {
        SimplifiedPredicates::Predicates(v) => v,
        SimplifiedPredicates::AlwaysFalse => vec![],
    };
    assert_eq!(once, twice);
}

/// T-PF-12  Predicates referencing different bindings but same field names → both unique.
#[test]
fn t_pf_12_same_field_different_bindings_both_kept() {
    let p1 = eq_pred("id", "A", "id");
    let p2 = eq_pred("id", "B", "id");
    let result = simplify_predicates(vec![p1.clone(), p2.clone()]);
    assert_eq!(result, SimplifiedPredicates::Predicates(vec![p1, p2]));
}

/// T-PF-13  Same binding, different inputs → both distinct.
#[test]
fn t_pf_13_same_binding_different_inputs_distinct() {
    let p1 = eq_pred("id1", "T", "id");
    let p2 = eq_pred("id2", "T", "id");
    let result = simplify_predicates(vec![p1.clone(), p2.clone()]);
    assert_eq!(result, SimplifiedPredicates::Predicates(vec![p1, p2]));
}

/// T-PF-14  MAX_SRPL_BODY_OPERATIONS distinct predicates are preserved.
#[test]
fn t_pf_14_many_distinct_predicates_all_preserved() {
    let preds: Vec<SrplPredicateIr> = (0..MAX_SRPL_BODY_OPERATIONS as u32)
        .map(|i| eq_pred(&format!("p{}", i), "T", &format!("f{}", i)))
        .collect();
    let expected_len = preds.len();
    let result = match simplify_predicates(preds) {
        SimplifiedPredicates::Predicates(v) => v,
        SimplifiedPredicates::AlwaysFalse => vec![],
    };
    assert_eq!(
        result.len(),
        expected_len,
        "all distinct predicates must survive"
    );
}

/// T-PF-15  GTE predicate with identical all-field duplicates → deduplicated.
#[test]
fn t_pf_15_gte_all_fields_duplicate_removed() {
    let p = gte_pred("Orders", "order_date", "start_date");
    let result = simplify_predicates(vec![p.clone(), p.clone(), p.clone()]);
    match result {
        SimplifiedPredicates::Predicates(v) => assert_eq!(v.len(), 1),
        SimplifiedPredicates::AlwaysFalse => panic!("unexpected AlwaysFalse"),
    }
}

/// T-PF-16  AlwaysFalse variant: current implementation does not produce it from
///          valid structural predicates (no type-range analysis yet). Guard this
///          contract so later range analysis does not regress.
#[test]
fn t_pf_16_no_spurious_always_false_for_valid_predicates() {
    let p1 = eq_pred("id", "T", "id");
    let p2 = gte_pred("T", "score", "min_score");
    let result = simplify_predicates(vec![p1, p2]);
    assert!(
        matches!(result, SimplifiedPredicates::Predicates(_)),
        "Valid predicates must never produce AlwaysFalse without range analysis"
    );
}

/// T-PF-17  Ordering of unique predicates is preserved (stable deduplication).
#[test]
fn t_pf_17_order_of_unique_predicates_is_stable() {
    let p1 = eq_pred("z", "T", "z");
    let p2 = eq_pred("a", "T", "a");
    let p3 = eq_pred("m", "T", "m");
    let result = match simplify_predicates(vec![p1.clone(), p2.clone(), p3.clone()]) {
        SimplifiedPredicates::Predicates(v) => v,
        SimplifiedPredicates::AlwaysFalse => panic!(),
    };
    // simplify_predicates does not re-sort; order must be insertion order for unique entries.
    assert_eq!(result, vec![p1, p2, p3]);
}

/// T-PF-18  Predicate key generation: EQ and GTE with same content have different keys.
#[test]
fn t_pf_18_eq_and_gte_on_same_fields_are_distinct() {
    let eq = eq_pred("t", "T", "t");
    let gte = gte_pred("T", "t", "t");
    let result = simplify_predicates(vec![eq.clone(), gte.clone()]);
    match result {
        SimplifiedPredicates::Predicates(v) => {
            assert_eq!(
                v.len(),
                2,
                "EQ and GTE are structurally different predicates"
            );
        },
        SimplifiedPredicates::AlwaysFalse => panic!(),
    }
}

/// T-PF-19  Empty result from full deduplication has Predicates([]), not AlwaysFalse.
#[test]
fn t_pf_19_single_duplicate_leaves_one_entry() {
    let p = eq_pred("x", "T", "x");
    match simplify_predicates(vec![p.clone(), p]) {
        SimplifiedPredicates::Predicates(v) => {
            assert_eq!(v.len(), 1, "one unique predicate must remain");
        },
        SimplifiedPredicates::AlwaysFalse => panic!("should not be AlwaysFalse"),
    }
}

/// T-PF-20  Simplify is safe with no-op on already-deduplicated input.
#[test]
fn t_pf_20_already_unique_input_unchanged() {
    let p1 = eq_pred("a", "T", "a");
    let p2 = eq_pred("b", "T", "b");
    let original = vec![p1.clone(), p2.clone()];
    let result = match simplify_predicates(original.clone()) {
        SimplifiedPredicates::Predicates(v) => v,
        SimplifiedPredicates::AlwaysFalse => panic!(),
    };
    assert_eq!(result, original);
}

/// T-NR-01  normalize deduplicates duplicate Read predicates.
#[test]
fn t_nr_01_normalize_deduplicates_read_predicates() {
    let p = eq_pred("id", "T", "id");
    let ir = make_ir(vec![read_op(0, "T", vec![p.clone(), p.clone()])]);
    let norm = normalize(ir).unwrap();
    match &norm.body.operations[0].kind {
        SrplBusinessOperationKindIr::Read { predicates, .. } => {
            assert_eq!(predicates.len(), 1);
        },
        _ => panic!("expected Read"),
    }
}

/// T-NR-02  normalize sorts Read predicates lexicographically.
#[test]
fn t_nr_02_normalize_sorts_read_predicates() {
    let p_b = eq_pred("b", "T", "b");
    let p_a = eq_pred("a", "T", "a");
    let ir = make_ir(vec![read_op(0, "T", vec![p_b, p_a])]);
    let norm = normalize(ir).unwrap();
    match &norm.body.operations[0].kind {
        SrplBusinessOperationKindIr::Read { predicates, .. } => {
            let keys: Vec<String> = predicates
                .iter()
                .map(|p| match p {
                    SrplPredicateIr::InputEqualsField {
                        binding,
                        field,
                        input,
                    } => format!("EQ:{}:{}:{}", binding, field, input),
                    SrplPredicateIr::FieldGreaterThanOrEqualInput {
                        binding,
                        field,
                        input,
                    } => format!("GTE:{}:{}:{}", binding, field, input),
                })
                .collect();
            let mut sorted = keys.clone();
            sorted.sort();
            assert_eq!(
                keys, sorted,
                "predicates must be in sorted order after normalize"
            );
        },
        _ => panic!(),
    }
}

/// T-NR-03  normalize re-indexes wrong ordinals to dense zero-based.
#[test]
fn t_nr_03_normalize_reindexes_ordinals() {
    let ir = make_ir(vec![
        SrplBusinessOperationIr {
            ordinal: 99,
            kind: read_op(99, "T", vec![]).kind,
        },
        SrplBusinessOperationIr {
            ordinal: 50,
            kind: raise_op(50).kind,
        },
    ]);
    let norm = normalize(ir).unwrap();
    for (i, op) in norm.body.operations.iter().enumerate() {
        assert_eq!(
            op.ordinal, i as u32,
            "ordinal must equal position after normalize"
        );
    }
}

/// T-NR-04  normalize is idempotent: normalize(normalize(ir)) == normalize(ir).
#[test]
fn t_nr_04_normalize_is_idempotent() {
    let p = eq_pred("id", "T", "id");
    let ir = make_ir(vec![read_op(0, "T", vec![p.clone(), p])]);
    let once = normalize(ir).unwrap();
    let twice = normalize(once.clone()).unwrap();
    assert_eq!(once, twice);
}

/// T-NR-05  normalize on empty body returns empty body without error.
#[test]
fn t_nr_05_normalize_empty_body_ok() {
    let ir = empty_ir();
    let norm = normalize(ir).unwrap();
    assert!(norm.body.operations.is_empty());
}

/// T-NR-06  normalize preserves Update assignment order.
#[test]
fn t_nr_06_normalize_preserves_update_assignment_order() {
    let ir = make_ir(vec![update_op(
        0,
        "db.ns.T",
        vec![],
        vec![
            simple_assignment("z_field", int(1)),
            simple_assignment("a_field", int(2)),
        ],
    )]);
    let norm = normalize(ir).unwrap();
    match &norm.body.operations[0].kind {
        SrplBusinessOperationKindIr::Update { assignments, .. } => {
            assert_eq!(assignments[0].field, "z_field");
            assert_eq!(assignments[1].field, "a_field");
        },
        _ => panic!(),
    }
}
