//! Bound SRPL body to typed IR lowering.

use andromeda_error::AndromedaResult;
use andromeda_srpl_ast::{BusinessOperationKindAst, ProcedureBodyAst};
use andromeda_srpl_ir::{
    Cardinality, ProcedureSignature, SrplAssignmentIr, SrplBusinessOperationIr,
    SrplBusinessOperationKindIr, SrplEmitValueIr, SrplPredicateIr, SrplProcedureBodyIr,
    SrplProcedureIr, SrplResultStreamIr, SrplValueIr,
};

/// Bound semantic input required by the lowering crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundProcedureLoweringInput {
    pub signature: ProcedureSignature,
    pub body: ProcedureBodyAst,
}

/// Lowers bound semantic procedure input to [`SrplProcedureIr`].
pub fn lower_bound_procedure(
    bound: BoundProcedureLoweringInput,
) -> AndromedaResult<SrplProcedureIr> {
    bound.signature.validate()?;
    let body = lower_body_ast(bound.body)?;
    Ok(SrplProcedureIr {
        name: bound.signature.name,
        inputs: bound.signature.accepts,
        result_streams: bound
            .signature
            .returns
            .into_iter()
            .map(|result| SrplResultStreamIr {
                name: result.name,
                cardinality: result.cardinality,
                columns: result.columns,
            })
            .collect(),
        body,
    })
}

/// Lowers a parsed procedure body to bounded SRPL body IR.
pub fn lower_body_ast(body: ProcedureBodyAst) -> AndromedaResult<SrplProcedureBodyIr> {
    let mut operations = Vec::new();
    for operation in body.operations {
        match operation.kind {
            BusinessOperationKindAst::Read {
                source,
                binding,
                cardinality,
            } => operations.push(SrplBusinessOperationIr {
                ordinal: operations.len() as u32,
                kind: SrplBusinessOperationKindIr::Read {
                    source: source.value,
                    binding: binding.value,
                    cardinality: cardinality.value,
                    predicates: Vec::new(),
                },
            }),
            BusinessOperationKindAst::Assert {
                predicate,
                failure_code,
            } => operations.push(SrplBusinessOperationIr {
                ordinal: operations.len() as u32,
                kind: SrplBusinessOperationKindIr::Assert {
                    predicate: SrplPredicateIr::InputEqualsField {
                        input: predicate.value,
                        binding: "scope".to_string(),
                        field: "value".to_string(),
                    },
                    failure_code: failure_code.value,
                },
            }),
            BusinessOperationKindAst::Update {
                target,
                mutation,
                affected_rows_exact,
            } => operations.push(SrplBusinessOperationIr {
                ordinal: operations.len() as u32,
                kind: SrplBusinessOperationKindIr::Update {
                    target: target.value,
                    predicates: Vec::new(),
                    assignments: vec![SrplAssignmentIr {
                        field: mutation.value,
                        value: SrplValueIr::Input("value".to_string()),
                    }],
                    affected_rows_exact: affected_rows_exact.map(|rows| rows.value),
                },
            }),
            BusinessOperationKindAst::Emit { stream, values } => {
                operations.push(SrplBusinessOperationIr {
                    ordinal: operations.len() as u32,
                    kind: SrplBusinessOperationKindIr::Emit {
                        stream: stream.value,
                        values: values
                            .into_iter()
                            .map(|value| SrplEmitValueIr {
                                column: value.value,
                                value: SrplValueIr::Bool(true),
                            })
                            .collect(),
                    },
                })
            }
            BusinessOperationKindAst::Raise { code } => operations.push(SrplBusinessOperationIr {
                ordinal: operations.len() as u32,
                kind: SrplBusinessOperationKindIr::Raise { code: code.value },
            }),
            BusinessOperationKindAst::Ensure {
                source,
                binding,
                lookup_input,
                lookup_field,
                quantity_field,
                quantity_input,
                failure_code,
            } => {
                operations.push(SrplBusinessOperationIr {
                    ordinal: operations.len() as u32,
                    kind: SrplBusinessOperationKindIr::Read {
                        source: source.value,
                        binding: binding.value.clone(),
                        cardinality: Cardinality::One,
                        predicates: vec![SrplPredicateIr::InputEqualsField {
                            input: lookup_input.value,
                            binding: binding.value.clone(),
                            field: lookup_field.value,
                        }],
                    },
                });
                operations.push(SrplBusinessOperationIr {
                    ordinal: operations.len() as u32,
                    kind: SrplBusinessOperationKindIr::Assert {
                        predicate: SrplPredicateIr::FieldGreaterThanOrEqualInput {
                            binding: binding.value,
                            field: quantity_field.value,
                            input: quantity_input.value,
                        },
                        failure_code: failure_code.value,
                    },
                });
            }
            BusinessOperationKindAst::UpdateSet {
                target,
                field,
                value_binding,
                value_field,
                value_input,
                where_input,
                where_binding,
                where_field,
                affected_rows_exact,
            } => operations.push(SrplBusinessOperationIr {
                ordinal: operations.len() as u32,
                kind: SrplBusinessOperationKindIr::Update {
                    target: target.value,
                    predicates: vec![SrplPredicateIr::InputEqualsField {
                        input: where_input.value,
                        binding: where_binding.value,
                        field: where_field.value,
                    }],
                    assignments: vec![SrplAssignmentIr {
                        field: field.value,
                        value: SrplValueIr::SubtractInput {
                            binding: value_binding.value,
                            field: value_field.value,
                            input: value_input.value,
                        },
                    }],
                    affected_rows_exact: Some(affected_rows_exact.value),
                },
            }),
            BusinessOperationKindAst::Return { stream, values } => {
                operations.push(SrplBusinessOperationIr {
                    ordinal: operations.len() as u32,
                    kind: SrplBusinessOperationKindIr::Emit {
                        stream: stream.value,
                        values: values
                            .into_iter()
                            .map(|value| SrplEmitValueIr {
                                column: value.value,
                                value: SrplValueIr::Bool(true),
                            })
                            .collect(),
                    },
                })
            }
        }
    }
    let ir = SrplProcedureBodyIr { operations };
    ir.validate_bounded()?;
    Ok(ir)
}
