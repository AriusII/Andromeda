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
    let cardinality = infer_cardinality_from_plan(body)?;
    let metadata = match analyze_body_for_row_count(body)? {
        RowCountInfo::Exact(count) => {
            ResultStreamMetadata::exact(result_stream.stream_id, column_count, cardinality, count)
        }
        RowCountInfo::Bounded(max) => {
            ResultStreamMetadata::bounded(result_stream.stream_id, column_count, cardinality, max)
        }
        RowCountInfo::Unknown => ResultStreamMetadata {
            stream_id: result_stream.stream_id,
            row_count_exact: None,
            row_count_max: None,
            column_count,
            cardinality,
        },
    };

    metadata.validate_before_payload()?;
    Ok(metadata)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowCountInfo {
    Exact(u64),
    Bounded(u64),
    Unknown,
}

fn analyze_body_for_row_count(body: &BoundSrplBodyPlan) -> AndromedaResult<RowCountInfo> {
    if body.operations.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            "procedure body must contain operations",
        ));
    }

    for operation in &body.operations {
        match operation {
            BoundSrplOperationPlan::ReadTable { cardinality, .. } => {
                return Ok(match cardinality {
                    Cardinality::One => RowCountInfo::Exact(1),
                    Cardinality::OptionalOne | Cardinality::Many | Cardinality::NonEmptyMany => {
                        RowCountInfo::Unknown
                    }
                });
            }
            BoundSrplOperationPlan::UpdateTable {
                affected_rows_exact,
                ..
            } => {
                return Ok(affected_rows_exact
                    .map(RowCountInfo::Exact)
                    .unwrap_or(RowCountInfo::Unknown));
            }
            BoundSrplOperationPlan::Emit { .. } => return Ok(RowCountInfo::Exact(1)),
            BoundSrplOperationPlan::Assert { .. } => {}
            BoundSrplOperationPlan::Raise { .. } => return Ok(RowCountInfo::Unknown),
        }
    }

    Ok(RowCountInfo::Unknown)
}

fn infer_cardinality_from_plan(body: &BoundSrplBodyPlan) -> AndromedaResult<Cardinality> {
    if body.operations.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            "procedure body must contain operations",
        ));
    }

    for operation in &body.operations {
        match operation {
            BoundSrplOperationPlan::Emit { values, .. } => {
                return Ok(if values.len() == 1 {
                    Cardinality::One
                } else {
                    Cardinality::Many
                });
            }
            BoundSrplOperationPlan::ReadTable { cardinality, .. } => return Ok(*cardinality),
            _ => {}
        }
    }

    Ok(Cardinality::Many)
}
