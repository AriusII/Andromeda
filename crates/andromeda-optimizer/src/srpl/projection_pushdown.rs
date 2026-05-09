use std::collections::BTreeSet;

use super::liveness::ColumnLiveness;
use andromeda_srpl_ir::{SrplBusinessOperationKindIr, SrplPredicateIr, SrplProcedureIr};

/// Effective column projection computed for one `ReadTable` operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveProjection {
    /// Binding name this projection applies to.
    pub binding: String,
    /// Columns that are live downstream or required by read-local predicates.
    /// `None` = projection not computed / all columns required.
    /// `Some(vec![])` = no columns required (count-only scan).
    /// `Some(vec![...])` = only these columns need to be fetched.
    pub live_columns: Option<Vec<String>>,
}

/// Result of the projection pushdown pass.
#[derive(Debug, Clone)]
pub struct ProjectionPushdownResult {
    /// The original IR, unchanged in structure.
    pub ir: SrplProcedureIr,
    /// One entry per `ReadTable` operation (same ordinal order).
    pub projections: Vec<EffectiveProjection>,
}

/// Apply projection pushdown to a procedure IR.
///
/// Does NOT mutate the `SrplProcedureIr` body structure — it only
/// computes and returns projection annotations. The storage layer
/// consumes the annotations at scan time.
pub fn apply(ir: SrplProcedureIr) -> ProjectionPushdownResult {
    let liveness = ColumnLiveness::compute(&ir);
    let mut projections = Vec::new();

    for op in &ir.body.operations {
        if let SrplBusinessOperationKindIr::Read {
            binding,
            predicates,
            ..
        } = &op.kind
        {
            let live = liveness.live_columns_after(op.ordinal);
            let mut live_for_binding: BTreeSet<String> = live
                .into_iter()
                .filter(|(b, _)| b == binding)
                .map(|(_, f)| f)
                .collect();
            add_read_predicate_fields(&mut live_for_binding, binding, predicates);

            projections.push(EffectiveProjection {
                binding: binding.clone(),
                live_columns: Some(live_for_binding.into_iter().collect()),
            });
        }
    }

    ProjectionPushdownResult { ir, projections }
}

fn add_read_predicate_fields(
    fields: &mut BTreeSet<String>,
    read_binding: &str,
    predicates: &[SrplPredicateIr],
) {
    for predicate in predicates {
        match predicate {
            SrplPredicateIr::InputEqualsField { binding, field, .. }
            | SrplPredicateIr::FieldGreaterThanOrEqualInput { binding, field, .. }
                if binding == read_binding =>
            {
                fields.insert(field.clone());
            },
            _ => {},
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_contract::QualifiedName;
    use andromeda_srpl_ir::{
        Cardinality, SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplProcedureBodyIr,
        SrplProcedureIr,
    };
    use andromeda_srpl_ir::{SrplEmitValueIr, SrplValueIr};

    fn qn(s: &str) -> QualifiedName {
        QualifiedName::parse(s).unwrap()
    }

    fn make_ir(ops: Vec<SrplBusinessOperationIr>) -> SrplProcedureIr {
        SrplProcedureIr {
            name: qn("db.ns.P"),
            inputs: vec![],
            result_streams: vec![],
            body: SrplProcedureBodyIr { operations: ops },
        }
    }

    // T-PJ-01: only id is emitted → projection = [id]
    #[test]
    fn unused_columns_are_excluded_from_projection() {
        let ir = make_ir(vec![
            SrplBusinessOperationIr {
                ordinal: 0,
                kind: SrplBusinessOperationKindIr::Read {
                    source: qn("db.ns.T"),
                    binding: "T".into(),
                    cardinality: Cardinality::One,
                    predicates: vec![],
                },
            },
            SrplBusinessOperationIr {
                ordinal: 1,
                kind: SrplBusinessOperationKindIr::Emit {
                    stream: "S".into(),
                    values: vec![SrplEmitValueIr {
                        column: "out_id".into(),
                        value: SrplValueIr::Field {
                            binding: "T".into(),
                            field: "id".into(),
                        },
                    }],
                },
            },
        ]);

        let result = apply(ir);
        assert_eq!(result.projections.len(), 1);
        let proj = &result.projections[0];
        assert_eq!(proj.binding, "T");
        let live = proj.live_columns.as_ref().unwrap();
        assert!(live.contains(&"id".to_string()), "id must be in projection");
        assert!(
            !live.contains(&"name".to_string()),
            "name must not be in projection"
        );
    }

    #[test]
    fn read_range_predicate_field_is_retained_without_downstream_use() {
        let ir = make_ir(vec![SrplBusinessOperationIr {
            ordinal: 0,
            kind: SrplBusinessOperationKindIr::Read {
                source: qn("db.ns.T"),
                binding: "T".into(),
                cardinality: Cardinality::One,
                predicates: vec![SrplPredicateIr::FieldGreaterThanOrEqualInput {
                    binding: "T".into(),
                    field: "qty".into(),
                    input: "requested_qty".into(),
                }],
            },
        }]);

        let result = apply(ir);
        assert_eq!(result.projections.len(), 1);
        assert_eq!(
            result.projections[0].live_columns.as_ref(),
            Some(&vec!["qty".to_string()])
        );
    }

    // T-PJ-05: no read → empty projections
    #[test]
    fn no_read_means_no_projections() {
        let ir = make_ir(vec![SrplBusinessOperationIr {
            ordinal: 0,
            kind: SrplBusinessOperationKindIr::Raise { code: "ERR".into() },
        }]);
        let result = apply(ir);
        assert!(result.projections.is_empty());
    }
}
