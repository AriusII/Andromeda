use std::collections::BTreeSet;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::ColumnDescriptor;

pub(crate) fn validate_columns(columns: &[ColumnDescriptor]) -> AndromedaResult<()> {
    validate_columns_with_min(columns, true)
}

fn validate_columns_with_min(
    columns: &[ColumnDescriptor],
    require_non_empty: bool,
) -> AndromedaResult<()> {
    if require_non_empty && columns.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "column list must not be empty",
        ));
    }

    let mut column_names = BTreeSet::new();
    for (expected_ordinal, column) in columns.iter().enumerate() {
        column.validate()?;
        if !column_names.insert(column.name.as_str()) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "column names must be unique",
            ));
        }
        if column.ordinal != expected_ordinal as u32 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "column ordinals must be dense and zero-based",
            ));
        }
    }

    Ok(())
}
