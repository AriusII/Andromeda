use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::ResultStreamMetadata;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResultValidationService;

impl ResultValidationService {
    pub fn validate_before_payload(metadata: ResultStreamMetadata) -> AndromedaResult<()> {
        if metadata.stream_id == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream metadata stream id must not be zero",
            ));
        }

        if metadata.column_count == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream metadata must declare at least one column",
            ));
        }

        if metadata.cardinality.requires_exact_row_count() && metadata.row_count_exact.is_none() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream requires RowCountExact before payload",
            ));
        }

        if let Some(row_count_exact) = metadata.row_count_exact
            && !metadata
                .cardinality
                .permits_exact_row_count(row_count_exact)
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream exact row count violates cardinality",
            ));
        }

        Ok(())
    }

    pub fn validate_completed_stream(
        metadata: ResultStreamMetadata,
        actual_row_count: u64,
    ) -> AndromedaResult<()> {
        Self::validate_before_payload(metadata)?;

        if let Some(row_count_exact) = metadata.row_count_exact
            && row_count_exact != actual_row_count
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream actual row count does not match RowCountExact",
            ));
        }

        if !metadata
            .cardinality
            .permits_exact_row_count(actual_row_count)
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream actual row count violates cardinality",
            ));
        }

        Ok(())
    }
}
