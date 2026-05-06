use andromeda_core::AndromedaResult;

use crate::{SrplProcedureBodyIr, SrplProcedureIr};

/// Apply predicate pushdown to a full procedure IR.
pub fn apply(mut ir: SrplProcedureIr) -> AndromedaResult<SrplProcedureIr> {
    ir.body = apply_to_body(ir.body)?;
    Ok(ir)
}

fn apply_to_body(body: SrplProcedureBodyIr) -> AndromedaResult<SrplProcedureBodyIr> {
    body.validate_bounded()?;
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::SrplPredicateIr;
    use crate::{Cardinality, SrplBusinessOperationIr, SrplBusinessOperationKindIr};
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

    // T-PP-01: assert failure semantics are not representable on Read, so the
    // safe pass must leave the Assert in place.
    #[test]
    fn assert_predicate_is_not_removed_from_upstream_read() {
        let pred = eq_pred("id", "T", "id");
        let ir = make_ir(vec![read_op(0, "T", vec![]), assert_op(1, pred.clone())]);
        let result = apply(ir).unwrap();
        assert_eq!(result.body.operations.len(), 2);
        match &result.body.operations[0].kind {
            SrplBusinessOperationKindIr::Read { predicates, .. } => {
                assert!(predicates.is_empty());
            }
            _ => panic!("expected Read"),
        }
        assert!(matches!(
            result.body.operations[1].kind,
            SrplBusinessOperationKindIr::Assert { .. }
        ));
    }

    // T-PP-05: empty body is a no-op
    #[test]
    fn empty_body_is_noop() {
        let ir = make_ir(vec![]);
        let result = apply(ir).unwrap();
        assert_eq!(result.body.operations.len(), 0);
    }

    // T-PP-08: ordinals remain dense and zero-based after the no-op pass
    #[test]
    fn ordinals_are_dense_after_safe_noop() {
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
