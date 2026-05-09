use std::collections::BTreeMap;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::values::StructuredObject;

/// Transaction-scoped state for SRPL execution.
#[derive(Debug, Clone, Default)]
pub struct SrplTransactionContext {
    pending_updates: BTreeMap<String, Vec<(usize, StructuredObject)>>,
    is_aborted: bool,
}

impl SrplTransactionContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_update(
        &mut self,
        table_id: &str,
        row_index: usize,
        updated_row: StructuredObject,
    ) -> AndromedaResult<()> {
        if self.is_aborted {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "SRPL transaction aborted; cannot record further updates",
            ));
        }

        self.pending_updates
            .entry(table_id.to_string())
            .or_default()
            .push((row_index, updated_row));

        Ok(())
    }

    pub fn abort(&mut self) {
        self.is_aborted = true;
    }

    pub fn is_aborted(&self) -> bool {
        self.is_aborted
    }

    pub fn pending_updates(&self) -> &BTreeMap<String, Vec<(usize, StructuredObject)>> {
        &self.pending_updates
    }
}
