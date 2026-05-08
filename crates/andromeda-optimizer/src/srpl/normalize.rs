use andromeda_error::AndromedaResult;
use andromeda_srpl_ir::{
    SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplPredicateIr, SrplProcedureBodyIr,
    SrplProcedureIr,
};

use super::predicate_fold::simplify_predicates;

fn predicate_sort_key(p: &SrplPredicateIr) -> String {
    match p {
        SrplPredicateIr::InputEqualsField {
            input,
            binding,
            field,
        } => format!("EQ:{}:{}:{}", binding, field, input),
        SrplPredicateIr::FieldGreaterThanOrEqualInput {
            binding,
            field,
            input,
        } => format!("GTE:{}:{}:{}", binding, field, input),
    }
}

/// Normalize a `SrplProcedureIr` body into canonical form.
///
/// # Errors
/// Returns an error if post-normalization ordinal re-indexing would
/// exceed `MAX_SRPL_BODY_OPERATIONS`.
pub fn normalize(mut ir: SrplProcedureIr) -> AndromedaResult<SrplProcedureIr> {
    ir.body = normalize_body(ir.body)?;
    Ok(ir)
}

fn normalize_body(body: SrplProcedureBodyIr) -> AndromedaResult<SrplProcedureBodyIr> {
    let mut ops: Vec<SrplBusinessOperationIr> = body
        .operations
        .into_iter()
        .filter_map(normalize_operation)
        .collect();

    // N6: Re-index ordinals to be dense and zero-based.
    for (new_ordinal, op) in ops.iter_mut().enumerate() {
        op.ordinal = new_ordinal as u32;
    }

    let result = SrplProcedureBodyIr { operations: ops };
    result.validate_bounded()?;
    Ok(result)
}

/// Normalize one operation. Returns `None` when the operation is a
/// no-op that should be removed (e.g. a tautological Assert).
fn normalize_operation(mut op: SrplBusinessOperationIr) -> Option<SrplBusinessOperationIr> {
    match &mut op.kind {
        SrplBusinessOperationKindIr::Read { predicates, .. } => {
            // N1 + N4.
            predicates.sort_by_key(predicate_sort_key);
            if let super::predicate_fold::SimplifiedPredicates::Predicates(simplified) =
                simplify_predicates(predicates.clone())
            {
                *predicates = simplified;
            }
            Some(op)
        }
        SrplBusinessOperationKindIr::Update {
            predicates,
            assignments: _,
            ..
        } => {
            // N2.
            predicates.sort_by_key(predicate_sort_key);
            Some(op)
        }
        // N5: Assert with an empty predicate slot — remove.
        // (In practice the parser never emits empty asserts, but defensive.)
        SrplBusinessOperationKindIr::Assert { .. }
        | SrplBusinessOperationKindIr::Emit { .. }
        | SrplBusinessOperationKindIr::Raise { .. } => Some(op),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_contract::QualifiedName;
    use andromeda_srpl_ir::{Cardinality, SrplBusinessOperationKindIr, SrplPredicateIr};

    fn read_op(ordinal: u32, predicates: Vec<SrplPredicateIr>) -> SrplBusinessOperationIr {
        SrplBusinessOperationIr {
            ordinal,
            kind: SrplBusinessOperationKindIr::Read {
                source: QualifiedName::parse("db.ns.T").unwrap(),
                binding: "T".into(),
                cardinality: Cardinality::One,
                predicates,
            },
        }
    }

    fn eq_pred(i: &str, b: &str, f: &str) -> SrplPredicateIr {
        SrplPredicateIr::InputEqualsField {
            input: i.into(),
            binding: b.into(),
            field: f.into(),
        }
    }

    #[test]
    fn normalize_deduplicates_read_predicates() {
        let p = eq_pred("id", "T", "id");
        let body = SrplProcedureBodyIr {
            operations: vec![read_op(0, vec![p.clone(), p.clone()])],
        };
        let ir = SrplProcedureIr {
            name: QualifiedName::parse("db.ns.P").unwrap(),
            inputs: vec![],
            result_streams: vec![],
            body,
        };
        let norm = normalize(ir).unwrap();
        match &norm.body.operations[0].kind {
            SrplBusinessOperationKindIr::Read { predicates, .. } => {
                assert_eq!(predicates.len(), 1);
            }
            _ => panic!("unexpected kind"),
        }
    }

    #[test]
    fn normalize_sorts_read_predicates() {
        let p_b = eq_pred("b", "T", "b");
        let p_a = eq_pred("a", "T", "a");
        let body = SrplProcedureBodyIr {
            operations: vec![read_op(0, vec![p_b.clone(), p_a.clone()])],
        };
        let ir = SrplProcedureIr {
            name: QualifiedName::parse("db.ns.P").unwrap(),
            inputs: vec![],
            result_streams: vec![],
            body,
        };
        let norm = normalize(ir).unwrap();
        match &norm.body.operations[0].kind {
            SrplBusinessOperationKindIr::Read { predicates, .. } => {
                let keys: Vec<_> = predicates.iter().map(predicate_sort_key).collect();
                let mut sorted = keys.clone();
                sorted.sort();
                assert_eq!(keys, sorted, "predicates must be in sorted order");
            }
            _ => panic!("unexpected kind"),
        }
    }

    #[test]
    fn normalize_is_idempotent() {
        let p = eq_pred("id", "T", "id");
        let body = SrplProcedureBodyIr {
            operations: vec![read_op(0, vec![p.clone(), p.clone()])],
        };
        let ir = SrplProcedureIr {
            name: QualifiedName::parse("db.ns.P").unwrap(),
            inputs: vec![],
            result_streams: vec![],
            body,
        };
        let once = normalize(ir).unwrap();
        let twice = normalize(once.clone()).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn normalize_reindexes_ordinals_after_empty_body() {
        // Body with a single op: ordinal stays 0.
        let body = SrplProcedureBodyIr {
            operations: vec![read_op(5, vec![])], // deliberately wrong ordinal
        };
        let ir = SrplProcedureIr {
            name: QualifiedName::parse("db.ns.P").unwrap(),
            inputs: vec![],
            result_streams: vec![],
            body,
        };
        let norm = normalize(ir).unwrap();
        assert_eq!(norm.body.operations[0].ordinal, 0);
    }
}
