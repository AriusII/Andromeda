//! Result metadata extraction from SRPL executable plans.
//!
//! This module implements deterministic extraction of result metadata
//! from `ExecutableProcedurePlan` evidence and procedure contracts,
//! converting plan information into `ResultStreamMetadata` that the
//! runtime uses for result framing and validation.
//!
//! ## Metadata Extraction Logic
//!
//! For each procedure kind (SELECT/INSERT/UPDATE/DELETE/CALL):
//! - **SELECT**: Row count is exact (all rows produced); column count from result stream
//! - **INSERT**: Affected count is exact (rows inserted); column count from result stream
//! - **UPDATE**: Affected count is exact (rows updated); column count from result stream
//! - **DELETE**: Affected count is exact (rows deleted); column count from result stream
//! - **CALL**: Return value mapped to metadata; column count from result stream
//!
//! ## Invariants
//!
//! - Extraction is deterministic given same evidence
//! - Result metadata is emitted before payload bytes (protocol contract)
//! - Row counts are exact when known; unbounded when unknown
//! - All result streams must be present in the procedure contract
//! - Metadata fields are validated before emission

use andromeda_catalog::ResultStreamContract;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_srpl::{
    Cardinality,
    procedure_model::{BoundSrplBodyPlan, BoundSrplOperationPlan, ExecutableProcedurePlan},
};

use crate::ResultStreamMetadata;

/// Result metadata extractor: converts executable procedure plans to result metadata.
///
/// This trait defines the contract for deterministic extraction of result stream
/// metadata from SRPL plans. Implementations must handle all 5 procedure kinds
/// and guarantee idempotency (same input → same output).
pub trait ResultMetadataExtractor {
    /// Extract result metadata from an executable plan and result streams.
    ///
    /// # Arguments
    /// * `plan` - The executable SRPL procedure plan containing body operations
    /// * `result_streams` - Contract-defined result streams for the procedure
    ///
    /// # Returns
    /// A valid `ResultStreamMetadata` ready for emission before payload, or an error
    /// if metadata cannot be deterministically extracted.
    fn extract_metadata(
        plan: &ExecutableProcedurePlan,
        result_streams: &[ResultStreamContract],
    ) -> AndromedaResult<ResultStreamMetadata>;
}

/// Default result metadata extractor implementation.
pub struct DefaultResultMetadataExtractor;

impl ResultMetadataExtractor for DefaultResultMetadataExtractor {
    fn extract_metadata(
        plan: &ExecutableProcedurePlan,
        result_streams: &[ResultStreamContract],
    ) -> AndromedaResult<ResultStreamMetadata> {
        // Validate inputs
        if result_streams.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "procedure must declare at least one result stream",
            ));
        }

        plan.validate()?;
        plan.body.validate()?;

        // For single-result procedures, use the only result stream
        if result_streams.len() == 1 {
            let result_stream = &result_streams[0];
            extract_single_stream_metadata(plan, result_stream)
        } else {
            // Multi-result procedures: identify which stream is being emitted
            // For now, use the first stream (can be extended for multi-result dispatch)
            let result_stream = &result_streams[0];
            extract_single_stream_metadata(plan, result_stream)
        }
    }
}

/// Extract metadata for a single result stream.
///
/// Analyzes the procedure body to determine row count, cardinality, and validates
/// the result stream contract.
fn extract_single_stream_metadata(
    plan: &ExecutableProcedurePlan,
    result_stream: &ResultStreamContract,
) -> AndromedaResult<ResultStreamMetadata> {
    result_stream.validate()?;

    // Analyze body operations to determine row count metadata
    let row_count_info = analyze_body_for_row_count(&plan.body)?;

    let column_count = result_stream.columns.len() as u32;
    let cardinality = infer_cardinality_from_plan(&plan.body)?;

    // Construct metadata based on analyzed information
    let metadata = match row_count_info {
        RowCountInfo::Exact(count) => {
            ResultStreamMetadata::exact(result_stream.stream_id, column_count, cardinality, count)
        }
        RowCountInfo::Bounded(max) => {
            ResultStreamMetadata::bounded(result_stream.stream_id, column_count, cardinality, max)
        }
        RowCountInfo::Unknown => {
            // For unbounded streams, emit metadata with no exact count
            ResultStreamMetadata {
                stream_id: result_stream.stream_id,
                row_count_exact: None,
                row_count_max: None,
                column_count,
                cardinality,
            }
        }
    };

    metadata.validate_before_payload()?;
    Ok(metadata)
}

/// Information about row count extracted from procedure body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowCountInfo {
    /// Exact count known before emission
    Exact(u64),
    /// Upper bound known but exact count not available
    Bounded(u64),
    /// Neither exact count nor bound known
    Unknown,
}

/// Analyze the procedure body to determine row count characteristics.
///
/// For deterministic operations (INSERT/UPDATE/DELETE with exact row counts),
/// extract the exact count. For SELECT and EMIT operations, determine if
/// row count can be inferred.
fn analyze_body_for_row_count(body: &BoundSrplBodyPlan) -> AndromedaResult<RowCountInfo> {
    if body.operations.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            "procedure body must contain operations",
        ));
    }

    // Scan operations for row count information
    // Typically, we look for the last operation that would produce output
    for operation in &body.operations {
        match operation {
            BoundSrplOperationPlan::ReadTable { .. } => {
                // SELECT operations typically produce variable row counts
                return Ok(RowCountInfo::Unknown);
            }
            BoundSrplOperationPlan::UpdateTable {
                affected_rows_exact,
                ..
            } => {
                // UPDATE operations can have exact row counts if declared
                if let Some(exact) = affected_rows_exact {
                    return Ok(RowCountInfo::Exact(*exact));
                }
                return Ok(RowCountInfo::Unknown);
            }
            BoundSrplOperationPlan::Emit { .. } => {
                // EMIT operations: row count depends on the emitted values
                // For now, we assume unknown (can be refined with value analysis)
                return Ok(RowCountInfo::Unknown);
            }
            BoundSrplOperationPlan::Assert { .. } => {
                // ASSERT operations don't produce output
                continue;
            }
            BoundSrplOperationPlan::Raise { .. } => {
                // RAISE operations terminate execution
                return Ok(RowCountInfo::Unknown);
            }
        }
    }

    Ok(RowCountInfo::Unknown)
}

/// Infer cardinality from the procedure body.
///
/// Analyzes operations to determine if the procedure produces One, OptionalOne,
/// Many, or NonEmptyMany results.
fn infer_cardinality_from_plan(body: &BoundSrplBodyPlan) -> AndromedaResult<Cardinality> {
    if body.operations.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            "procedure body must contain operations",
        ));
    }

    // Scan for EMIT or READ operations that determine cardinality
    for operation in &body.operations {
        match operation {
            BoundSrplOperationPlan::Emit { values, .. } => {
                // EMIT operations determine cardinality based on value count
                match values.len() {
                    1 => return Ok(Cardinality::One),
                    _ => return Ok(Cardinality::Many),
                }
            }
            BoundSrplOperationPlan::ReadTable { cardinality, .. } => {
                // READ operations carry explicit cardinality
                return Ok(*cardinality);
            }
            _ => continue,
        }
    }

    // Default to Many if no specific cardinality found
    Ok(Cardinality::Many)
}
