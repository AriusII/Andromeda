use super::liveness::ColumnLiveness;
use crate::{SrplBusinessOperationKindIr, SrplProcedureIr};

/// Effective column projection computed for one `ReadTable` operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveProjection {
    /// Binding name this projection applies to.
    pub binding: String,
    /// Columns that are live downstream.
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
        if let SrplBusinessOperationKindIr::Read { binding, .. } = &op.kind {
            let live = liveness.live_columns_after(op.ordinal);
            let live_for_binding: Vec<String> = live
                .into_iter()
                .filter(|(b, _)| b == binding)
                .map(|(_, f)| f)
                .collect();

            projections.push(EffectiveProjection {
                binding: binding.clone(),
                live_columns: Some(live_for_binding),
            });
        }
    }

    ProjectionPushdownResult { ir, projections }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{SrplEmitValueIr, SrplValueIr};
    use crate::{
        Cardinality, SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplProcedureBodyIr,
        SrplProcedureIr,
    };
    use andromeda_catalog::QualifiedName;

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
