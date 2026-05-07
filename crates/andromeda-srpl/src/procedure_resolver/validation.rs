use std::collections::BTreeSet;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, ColumnDescriptor};

pub(super) fn validate_dense_columns_allow_empty(
    columns: &[ColumnDescriptor],
    context: &str,
) -> AndromedaResult<()> {
    let mut names = BTreeSet::new();
    for (expected_ordinal, column) in columns.iter().enumerate() {
        column.validate()?;
        if !names.insert(column.name.as_str()) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                format!("{context} names must be unique"),
            ));
        }
        if column.ordinal != expected_ordinal as u32 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                format!("{context} must be dense and zero-based"),
            ));
        }
    }
    Ok(())
}
