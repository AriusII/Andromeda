use andromeda_core::AndromedaResult;

use crate::ir::SrplPredicateIr;
use crate::{
    SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplProcedureBodyIr, SrplProcedureIr,
};

/// Apply predicate pushdown to a full procedure IR.
pub fn apply(mut ir: SrplProcedureIr) -> AndromedaResult<SrplProcedureIr> {
    ir.body = apply_to_body(ir.body)?;
    Ok(ir)
}

fn apply_to_body(body: SrplProcedureBodyIr) -> AndromedaResult<SrplProcedureBodyIr> {
    let mut ops = body.operations;

    // Build a quick map: binding_name → index of its Read operation.
    // We re-compute this after every modification since the vec shrinks.
    loop {
        let changed = try_push_one_assert(&mut ops);
        if !changed {
            break;
        }
    }

    // Re-index ordinals (INV-11).
    for (i, op) in ops.iter_mut().enumerate() {
        op.ordinal = i as u32;
    }

    let result = SrplProcedureBodyIr { operations: ops };
    result.validate_bounded()?;
    Ok(result)
}

/// Attempt to push exactly one eligible Assert into its upstream Read.
/// Returns `true` if a push was performed (loop continues), `false` if
/// no more pushes are possible.
fn try_push_one_assert(ops: &mut Vec<SrplBusinessOperationIr>) -> bool {
    for assert_idx in 0..ops.len() {
        let (assert_pred, assert_binding) = match &ops[assert_idx].kind {
            SrplBusinessOperationKindIr::Assert { predicate, .. } => {
                let binding = predicate_binding(predicate);
                (predicate.clone(), binding)
            }
            _ => continue,
        };

        let binding = match assert_binding {
            Some(b) => b,
            None => continue, // E1: no clear binding
        };

        let read_idx = ops[..assert_idx]
            .iter()
            .rposition(|op| matches_read_binding(op, &binding));

        let read_idx = match read_idx {
            Some(i) => i,
            None => continue, // E1: no upstream Read
        };

        let read_source = read_source_name(&ops[read_idx]);
        let has_intervening_mutation = ops[read_idx + 1..assert_idx]
            .iter()
            .any(|op| is_update_on(op, &read_source));
        if has_intervening_mutation {
            continue; // E5 violation
        }

        if is_correlated(&assert_pred, ops, assert_idx) {
            continue; // E3 violation
        }

        // Eligible — push the predicate and remove the Assert.
        push_predicate_to_read(&mut ops[read_idx], assert_pred);
        ops.remove(assert_idx);
        return true; // a push occurred; restart the scan
    }
    false
}

/// Extract the binding name from a predicate (E1 check).
fn predicate_binding(pred: &SrplPredicateIr) -> Option<String> {
    match pred {
        SrplPredicateIr::InputEqualsField { binding, .. }
        | SrplPredicateIr::FieldGreaterThanOrEqualInput { binding, .. } => Some(binding.clone()),
    }
}

fn matches_read_binding(op: &SrplBusinessOperationIr, binding: &str) -> bool {
    matches!(&op.kind, SrplBusinessOperationKindIr::Read { binding: b, .. } if b == binding)
}

fn read_source_name(op: &SrplBusinessOperationIr) -> String {
    match &op.kind {
        SrplBusinessOperationKindIr::Read { source, .. } => source.as_catalog_path(),
        _ => String::new(),
    }
}

fn is_update_on(op: &SrplBusinessOperationIr, source: &str) -> bool {
    matches!(&op.kind,
        SrplBusinessOperationKindIr::Update { target, .. }
        if target.as_catalog_path() == source
    )
}

/// E3: A predicate is correlated if its `input` field matches the name
/// of any Read binding *other than* the target Read.
fn is_correlated(
    pred: &SrplPredicateIr,
    ops: &[SrplBusinessOperationIr],
    assert_idx: usize,
) -> bool {
    let pred_input = match pred {
        SrplPredicateIr::InputEqualsField { input, .. }
        | SrplPredicateIr::FieldGreaterThanOrEqualInput { input, .. } => input,
    };
    let pred_binding = match pred {
        SrplPredicateIr::InputEqualsField { binding, .. }
        | SrplPredicateIr::FieldGreaterThanOrEqualInput { binding, .. } => binding,
    };
    // Check if `input` is actually a binding name of another Read.
    ops[..assert_idx].iter().any(|op| {
        matches!(&op.kind,
            SrplBusinessOperationKindIr::Read { binding: b, .. }
            if b != pred_binding && b == pred_input
        )
    })
}

fn push_predicate_to_read(op: &mut SrplBusinessOperationIr, pred: SrplPredicateIr) {
    if let SrplBusinessOperationKindIr::Read { predicates, .. } = &mut op.kind {
        // Only add if not already present (idempotency guard).
        let key = pred_key(&pred);
        if !predicates.iter().any(|p| pred_key(p) == key) {
            predicates.push(pred);
        }
    }
}

fn pred_key(p: &SrplPredicateIr) -> String {
    match p {
        SrplPredicateIr::InputEqualsField {
            input,
            binding,
            field,
        } => format!("EQ:{}:{}:{}", input, binding, field),
        SrplPredicateIr::FieldGreaterThanOrEqualInput {
            binding,
            field,
            input,
        } => format!("GTE:{}:{}:{}", binding, field, input),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::SrplPredicateIr;
    use crate::{Cardinality, SrplBusinessOperationKindIr};
    use andromeda_catalog::QualifiedName;

    fn qn(s: &str) -> QualifiedName {
        QualifiedName::parse(s).unwrap()
    }

    fn read_op(
        ordinal: u32,
        binding: &str,
        predicates: Vec<SrplPredicateIr>,
    ) -> SrplBusinessOperationIr {
        SrplBusinessOperationIr {
            ordinal,
            kind: SrplBusinessOperationKindIr::Read {
                source: qn("db.ns.T"),
                binding: binding.into(),
                cardinality: Cardinality::One,
                predicates,
            },
        }
    }

    fn assert_op(ordinal: u32, pred: SrplPredicateIr) -> SrplBusinessOperationIr {
        SrplBusinessOperationIr {
            ordinal,
            kind: SrplBusinessOperationKindIr::Assert {
                predicate: pred,
                failure_code: "ERR".into(),
            },
        }
    }

    fn eq_pred(input: &str, binding: &str, field: &str) -> SrplPredicateIr {
        SrplPredicateIr::InputEqualsField {
            input: input.into(),
            binding: binding.into(),
            field: field.into(),
        }
    }

    fn make_ir(ops: Vec<SrplBusinessOperationIr>) -> SrplProcedureIr {
        SrplProcedureIr {
            name: qn("db.ns.P"),
            inputs: vec![],
            result_streams: vec![],
            body: SrplProcedureBodyIr { operations: ops },
        }
    }

    // T-PP-01: basic push
    #[test]
    fn assert_predicate_is_pushed_into_upstream_read() {
        let pred = eq_pred("id", "T", "id");
        let ir = make_ir(vec![read_op(0, "T", vec![]), assert_op(1, pred.clone())]);
        let result = apply(ir).unwrap();
        assert_eq!(result.body.operations.len(), 1);
        match &result.body.operations[0].kind {
            SrplBusinessOperationKindIr::Read { predicates, .. } => {
                assert_eq!(predicates.len(), 1);
                assert_eq!(predicates[0], pred);
            }
            _ => panic!("expected Read"),
        }
    }

    // T-PP-05: empty body is a no-op
    #[test]
    fn empty_body_is_noop() {
        let ir = make_ir(vec![]);
        let result = apply(ir).unwrap();
        assert_eq!(result.body.operations.len(), 0);
    }

    // T-PP-08: ordinals remain dense and zero-based after push
    #[test]
    fn ordinals_are_dense_after_push() {
        let pred = eq_pred("id", "T", "id");
        let ir = make_ir(vec![read_op(0, "T", vec![]), assert_op(1, pred)]);
        let result = apply(ir).unwrap();
        for (i, op) in result.body.operations.iter().enumerate() {
            assert_eq!(op.ordinal, i as u32);
        }
    }

    // T-PP-07: assert where binding has no upstream Read → not pushed
    #[test]
    fn assert_without_upstream_read_stays() {
        let pred = eq_pred("id", "UNKNOWN_BINDING", "id");
        let ir = make_ir(vec![assert_op(0, pred.clone())]);
        let result = apply(ir).unwrap();
        assert_eq!(result.body.operations.len(), 1);
        assert!(matches!(
            result.body.operations[0].kind,
            SrplBusinessOperationKindIr::Assert { .. }
        ));
    }
}
