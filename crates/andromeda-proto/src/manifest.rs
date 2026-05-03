use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ColumnDescriptor,
    ContractHash, ProcedureId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolLayout {
    pub descriptor_set_hash: ContractHash,
    pub frame_envelope_hash: ContractHash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureManifest {
    pub procedure_id: ProcedureId,
    pub procedure_name: String,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
    pub protocol_layout: ProtocolLayout,
}

impl ProcedureManifest {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.procedure_name.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure manifest name must not be empty",
            ));
        }

        if self.contract_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure manifest contract hash must not be zero",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultStreamDescriptor {
    pub stream_name: String,
    pub columns: Vec<ColumnDescriptor>,
    pub row_count_exact: Option<u64>,
}

impl ResultStreamDescriptor {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.stream_name.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream descriptor name must not be empty",
            ));
        }

        for column in &self.columns {
            column.validate()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::{ScalarType, TypeDescriptor};

    #[test]
    fn result_descriptor_validates_columns() {
        let descriptor = ResultStreamDescriptor {
            stream_name: "Reservation".to_string(),
            columns: vec![ColumnDescriptor {
                name: "ProductId".to_string(),
                data_type: TypeDescriptor::required(ScalarType::I64),
                ordinal: 0,
            }],
            row_count_exact: Some(1),
        };

        assert!(descriptor.validate().is_ok());
    }
}
