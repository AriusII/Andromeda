use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultRowCountSummary {
    pub result_name: String,
    pub rows_emitted: u64,
    pub row_count_exact: Option<u64>,
}

impl ResultRowCountSummary {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.result_name.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result row-count summary name must not be empty",
            ));
        }

        if let Some(exact) = self.row_count_exact
            && exact != self.rows_emitted
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "exact result row count must match emitted rows",
            ));
        }

        Ok(())
    }
}
