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

        if let Some(row_count_exact) = metadata.row_count_exact {
            if !metadata
                .cardinality
                .permits_exact_row_count(row_count_exact)
            {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "result stream exact row count violates cardinality",
                ));
            }
        }

        // Enforce row_count_max consistency with the declared cardinality
        // and any declared exact row count. This is the V0 *bounded
        // cardinality* gate: One/OptionalOne are intrinsically bounded at
        // 1, and Many/NonEmptyMany may only declare a bound that is
        // consistent with their minimum row count. When both `row_count_exact`
        // and `row_count_max` are present the bound must dominate the exact
        // count.
        if let Some(row_count_max) = metadata.row_count_max {
            if !metadata.cardinality.permits_row_count_max(row_count_max) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "result stream row_count_max violates cardinality bounds",
                ));
            }
            if let Some(row_count_exact) = metadata.row_count_exact {
                if row_count_exact > row_count_max {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Contract,
                        "result stream row_count_exact exceeds declared row_count_max",
                    ));
                }
            }
        }

        Ok(())
    }

    pub fn validate_completed_stream(
        metadata: ResultStreamMetadata,
        actual_row_count: u64,
    ) -> AndromedaResult<()> {
        Self::validate_before_payload(metadata)?;

        if let Some(row_count_exact) = metadata.row_count_exact {
            if row_count_exact != actual_row_count {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "result stream actual row count does not match RowCountExact",
                ));
            }
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

        if let Some(row_count_max) = metadata.row_count_max {
            if actual_row_count > row_count_max {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "result stream actual row count exceeds declared row_count_max",
                ));
            }
        }

        Ok(())
    }
}
