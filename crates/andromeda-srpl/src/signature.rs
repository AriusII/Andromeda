use andromeda_catalog::{ProcedureContractRef, QualifiedName};
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, ColumnDescriptor};

use crate::Cardinality;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureSignature {
    pub name: QualifiedName,
    pub accepts: Vec<ColumnDescriptor>,
    pub returns: Vec<ResultContract>,
}

impl ProcedureSignature {
    pub fn validate(&self) -> AndromedaResult<()> {
        for parameter in &self.accepts {
            parameter.validate()?;
        }

        for result in &self.returns {
            result.validate()?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultContract {
    pub name: String,
    pub cardinality: Cardinality,
    pub columns: Vec<ColumnDescriptor>,
}

impl ResultContract {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.name.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL result contract name must not be empty",
            ));
        }

        for column in &self.columns {
            column.validate()?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstNodePlaceholder {
    pub concept: String,
    pub cardinality: Cardinality,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticIrPlaceholder {
    pub procedure: ProcedureContractRef,
    pub result_cardinality: Cardinality,
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::{ScalarType, TimestampType, TypeDescriptor};

    #[test]
    fn procedure_signature_validates_contract_shapes() {
        let signature = ProcedureSignature {
            name: QualifiedName::parse("Inventory.ReserveStock").unwrap(),
            accepts: vec![ColumnDescriptor {
                name: "ProductId".to_string(),
                data_type: TypeDescriptor::required(ScalarType::I64),
                ordinal: 0,
            }],
            returns: vec![ResultContract {
                name: "Reservation".to_string(),
                cardinality: Cardinality::One,
                columns: vec![ColumnDescriptor {
                    name: "ReservedAt".to_string(),
                    data_type: TypeDescriptor::required(ScalarType::Timestamp(
                        TimestampType::Transaction,
                    )),
                    ordinal: 0,
                }],
            }],
        };

        assert!(signature.validate().is_ok());
    }
}
