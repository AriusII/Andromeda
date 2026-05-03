use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, ContractHash};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuredObjectLayout {
    RowMajor,
    ColumnMajor,
    Hybrid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredObjectHeader {
    pub name: String,
    pub contract_hash: ContractHash,
    pub row_count_exact: u64,
    pub column_count: u32,
    pub layout: StructuredObjectLayout,
    pub payload_length: u64,
}

impl StructuredObjectHeader {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.name.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "StructuredObject name must not be empty",
            ));
        }

        if self.contract_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "StructuredObject contract hash must not be zero",
            ));
        }

        if self.column_count == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "StructuredObject must declare at least one column",
            ));
        }

        Ok(())
    }
}
