use crate::procedure_model::Cardinality;

use super::{contracts::SrplRowBound, diagnostics::SrplExecutionFailure};

/// Result of a typed assertion adapter call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SrplAssertResult {
    pub passed: bool,
}

impl SrplAssertResult {
    pub const fn new(passed: bool) -> Self {
        Self { passed }
    }
}

/// Result of a typed read/scan adapter call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplReadResult<Row> {
    pub rows: Vec<Row>,
}

impl<Row> SrplReadResult<Row> {
    pub fn new(
        rows: Vec<Row>,
        cardinality: Cardinality,
        bound: SrplRowBound,
    ) -> Result<Self, SrplExecutionFailure> {
        validate_actual_rows(rows.len() as u64, cardinality, bound)?;
        Ok(Self { rows })
    }
}

/// Result of a typed update adapter call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SrplUpdateResult {
    pub affected_rows: u64,
}

impl SrplUpdateResult {
    pub fn new(affected_rows: u64, bound: SrplRowBound) -> Result<Self, SrplExecutionFailure> {
        if !bound.permits_actual(affected_rows) {
            return Err(SrplExecutionFailure::CardinalityViolation {
                expected: Cardinality::Many,
                bound,
                actual_rows: affected_rows,
            });
        }
        Ok(Self { affected_rows })
    }
}

/// Result of a typed emit adapter call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SrplEmitResult {
    pub emitted_rows: u64,
}

impl SrplEmitResult {
    pub fn new(
        emitted_rows: u64,
        cardinality: Cardinality,
        bound: SrplRowBound,
    ) -> Result<Self, SrplExecutionFailure> {
        validate_actual_rows(emitted_rows, cardinality, bound)?;
        Ok(Self { emitted_rows })
    }
}

fn validate_actual_rows(
    actual_rows: u64,
    cardinality: Cardinality,
    bound: SrplRowBound,
) -> Result<(), SrplExecutionFailure> {
    if cardinality.permits_exact_row_count(actual_rows) && bound.permits_actual(actual_rows) {
        return Ok(());
    }

    Err(SrplExecutionFailure::CardinalityViolation {
        expected: cardinality,
        bound,
        actual_rows,
    })
}
