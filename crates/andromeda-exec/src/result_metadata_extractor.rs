//! Result metadata extraction from SRPL executable plans.

use andromeda_catalog::ResultStreamContract;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_srpl::{
    Cardinality,
    procedure_model::{BoundSrplBodyPlan, BoundSrplOperationPlan, ExecutableProcedurePlan},
};

use crate::ResultStreamMetadata;

pub trait ResultMetadataExtractor {
    fn extract_metadata(
        plan: &ExecutableProcedurePlan,
        result_streams: &[ResultStreamContract],
    ) -> AndromedaResult<ResultStreamMetadata>;
}

pub struct DefaultResultMetadataExtractor;

impl ResultMetadataExtractor for DefaultResultMetadataExtractor {
    fn extract_metadata(
        plan: &ExecutableProcedurePlan,
        result_streams: &[ResultStreamContract],
    ) -> AndromedaResult<ResultStreamMetadata> {
        if result_streams.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "procedure must declare at least one result stream",
            ));
        }

        plan.validate()?;
        plan.body.validate()?;

        extract_single_stream_metadata(&plan.body, &result_streams[0])
    }
}

fn extract_single_stream_metadata(
    body: &BoundSrplBodyPlan,
    result_stream: &ResultStreamContract,
) -> AndromedaResult<ResultStreamMetadata> {
    result_stream.validate()?;

    let column_count = result_stream.columns.len() as u32;
    let shape = infer_result_stream_shape(body, result_stream)?;
    let metadata = match shape.row_count {
        RowCountInfo::Exact(count) => ResultStreamMetadata::exact(
            result_stream.stream_id,
            column_count,
            shape.cardinality,
            count,
        ),
        RowCountInfo::Unknown => ResultStreamMetadata {
            stream_id: result_stream.stream_id,
            row_count_exact: None,
            row_count_max: None,
            column_count,
            cardinality: shape.cardinality,
        },
    };

    metadata.validate_before_payload()?;
    Ok(metadata)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResultStreamShape {
    cardinality: Cardinality,
    row_count: RowCountInfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowCountInfo {
    Exact(u64),
    Unknown,
}

fn infer_result_stream_shape(
    body: &BoundSrplBodyPlan,
    result_stream: &ResultStreamContract,
) -> AndromedaResult<ResultStreamShape> {
    if body.operations.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            "procedure body must contain operations",
        ));
    }

    // An EMIT naming the selected result stream is authoritative: earlier
    // reads or updates are intermediate work, not result payload metadata.
    for operation in &body.operations {
        if let BoundSrplOperationPlan::Emit { stream, .. } = operation
            && stream == &result_stream.name
        {
            return Ok(ResultStreamShape {
                cardinality: Cardinality::One,
                row_count: RowCountInfo::Exact(1),
            });
        }
    }

    for operation in &body.operations {
        match operation {
            BoundSrplOperationPlan::ReadTable { cardinality, .. } => {
                return Ok(ResultStreamShape {
                    cardinality: *cardinality,
                    row_count: match cardinality {
                        Cardinality::One => RowCountInfo::Exact(1),
                        Cardinality::OptionalOne
                        | Cardinality::Many
                        | Cardinality::NonEmptyMany => RowCountInfo::Unknown,
                    },
                });
            }
            BoundSrplOperationPlan::UpdateTable {
                affected_rows_exact,
                ..
            } => {
                return Ok(ResultStreamShape {
                    cardinality: Cardinality::Many,
                    row_count: affected_rows_exact
                        .map(RowCountInfo::Exact)
                        .unwrap_or(RowCountInfo::Unknown),
                });
            }
            BoundSrplOperationPlan::Raise { .. } => {
                return Ok(ResultStreamShape {
                    cardinality: Cardinality::Many,
                    row_count: RowCountInfo::Unknown,
                });
            }
            BoundSrplOperationPlan::Assert { .. } | BoundSrplOperationPlan::Emit { .. } => {}
        }
    }

    Err(AndromedaError::new(
        AndromedaErrorKind::Srpl,
        "procedure body must contain a result-producing operation",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_srpl::procedure_model::{SrplEmitValueIr, SrplValueIr};

    fn body_with(operations: Vec<BoundSrplOperationPlan>) -> BoundSrplBodyPlan {
        BoundSrplBodyPlan { operations }
    }

    fn result_stream(name: &str) -> ResultStreamContract {
        ResultStreamContract {
            stream_id: 1,
            name: name.to_string(),
            columns: vec![andromeda_core::ColumnDescriptor {
                name: "value".to_string(),
                ordinal: 0,
                data_type: andromeda_core::TypeDescriptor::required(
                    andromeda_core::ScalarType::Bool,
                ),
            }],
            row_count_exact_required: false,
        }
    }

    #[test]
    fn matching_emit_shape_is_one_row_even_after_intermediate_operations() {
        let body = body_with(vec![
            BoundSrplOperationPlan::ReadTable {
                ordinal: 0,
                source: andromeda_catalog::CatalogObjectRef {
                    object_id: andromeda_core::CatalogObjectId::new(2),
                    name: andromeda_catalog::QualifiedName::parse("test.table").unwrap(),
                    kind: andromeda_catalog::ObjectKind::Table,
                    catalog_version: andromeda_core::CatalogVersion::new(1),
                },
                binding: "t".to_string(),
                cardinality: Cardinality::Many,
                predicates: vec![],
            },
            BoundSrplOperationPlan::Emit {
                ordinal: 1,
                stream: "result".to_string(),
                values: emit_values(),
            },
        ]);

        let shape = infer_result_stream_shape(&body, &result_stream("result")).unwrap();

        assert_eq!(shape.cardinality, Cardinality::One);
        assert_eq!(shape.row_count, RowCountInfo::Exact(1));
    }

    #[test]
    fn unmatched_emit_is_not_used_as_selected_stream_metadata() {
        let body = body_with(vec![BoundSrplOperationPlan::Emit {
            ordinal: 0,
            stream: "audit".to_string(),
            values: emit_values(),
        }]);

        assert!(infer_result_stream_shape(&body, &result_stream("result")).is_err());
    }

    fn emit_values() -> Vec<SrplEmitValueIr> {
        vec![SrplEmitValueIr {
            column: "value".to_string(),
            value: SrplValueIr::Bool(true),
        }]
    }
}
