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
    pub shape_hash: ContractHash,
    pub row_count_exact: u64,
    pub column_count: u32,
    pub layout: StructuredObjectLayout,
    pub payload_length: u64,
    pub payload_checksum: Option<u64>,
    pub max_payload_length: Option<u64>,
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

        if self.shape_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "StructuredObject shape hash must not be zero",
            ));
        }

        if self.column_count == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "StructuredObject must declare at least one column",
            ));
        }

        if let Some(max_payload_length) = self.max_payload_length {
            if self.payload_length > max_payload_length {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Protocol,
                    "StructuredObject payload length exceeds declared bound",
                ));
            }
        }

        Ok(())
    }
}
