use andromeda_catalog::{ProcedureContractRef, QualifiedName};
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, ColumnDescriptor};
use std::collections::BTreeSet;

use crate::Cardinality;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureSignature {
    pub name: QualifiedName,
    pub accepts: Vec<ColumnDescriptor>,
    pub returns: Vec<ResultContract>,
}

impl ProcedureSignature {
    pub fn validate(&self) -> AndromedaResult<()> {
        validate_dense_columns(&self.accepts, false, "SRPL parameter ordinals")?;

        let mut result_names = BTreeSet::new();
        for result in &self.returns {
            if !result_names.insert(result.name.as_str()) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Srpl,
                    "SRPL result stream names must be unique",
                ));
            }
            result.validate()?;
        }

        Ok(())
    }

    pub fn validate_against_contract_ref(
        &self,
        contract_ref: &ProcedureContractRef,
    ) -> AndromedaResult<()> {
        self.validate()?;
        contract_ref.validate()?;

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

        validate_dense_columns(&self.columns, true, "SRPL result column ordinals")?;

        Ok(())
    }
}

fn validate_dense_columns(
    columns: &[ColumnDescriptor],
    require_non_empty: bool,
    context: &str,
) -> AndromedaResult<()> {
    if require_non_empty && columns.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            "SRPL result contract must declare at least one column",
        ));
    }

    let mut column_names = BTreeSet::new();
    for (expected_ordinal, column) in columns.iter().enumerate() {
        column.validate()?;
        if !column_names.insert(column.name.as_str()) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                format!("{context} names must be unique"),
            ));
        }
        if column.ordinal != expected_ordinal as u32 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                format!("{context} must be dense and zero-based"),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::{
        CatalogVersion, ContractHash, ProcedureId, ScalarType, TimestampType, TypeDescriptor,
    };

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

    #[test]
    fn procedure_signature_rejects_sparse_parameter_ordinals() {
        let signature = ProcedureSignature {
            name: QualifiedName::parse("Inventory.ReserveStock").unwrap(),
            accepts: vec![ColumnDescriptor {
                name: "ProductId".to_string(),
                data_type: TypeDescriptor::required(ScalarType::I64),
                ordinal: 1,
            }],
            returns: vec![ResultContract {
                name: "Reservation".to_string(),
                cardinality: Cardinality::One,
                columns: vec![ColumnDescriptor {
                    name: "Reserved".to_string(),
                    data_type: TypeDescriptor::required(ScalarType::Bool),
                    ordinal: 0,
                }],
            }],
        };

        let error = signature.validate().unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
        assert!(error.message().contains("dense"));
    }

    #[test]
    fn result_contract_rejects_empty_or_sparse_columns() {
        let empty = ResultContract {
            name: "Reservation".to_string(),
            cardinality: Cardinality::One,
            columns: Vec::new(),
        };

        assert_eq!(
            empty.validate().unwrap_err().kind(),
            AndromedaErrorKind::Srpl
        );

        let sparse = ResultContract {
            name: "Reservation".to_string(),
            cardinality: Cardinality::One,
            columns: vec![ColumnDescriptor {
                name: "Reserved".to_string(),
                data_type: TypeDescriptor::required(ScalarType::Bool),
                ordinal: 2,
            }],
        };

        let error = sparse.validate().unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
        assert!(error.message().contains("dense"));
    }

    #[test]
    fn procedure_signature_rejects_duplicate_parameter_names() {
        let signature = ProcedureSignature {
            name: QualifiedName::parse("Inventory.ReserveStock").unwrap(),
            accepts: vec![
                ColumnDescriptor {
                    name: "ProductId".to_string(),
                    data_type: TypeDescriptor::required(ScalarType::I64),
                    ordinal: 0,
                },
                ColumnDescriptor {
                    name: "ProductId".to_string(),
                    data_type: TypeDescriptor::required(ScalarType::I64),
                    ordinal: 1,
                },
            ],
            returns: vec![ResultContract {
                name: "Reservation".to_string(),
                cardinality: Cardinality::One,
                columns: vec![ColumnDescriptor {
                    name: "Reserved".to_string(),
                    data_type: TypeDescriptor::required(ScalarType::Bool),
                    ordinal: 0,
                }],
            }],
        };

        let error = signature.validate().unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
        assert!(error.message().contains("unique"));
    }

    #[test]
    fn procedure_signature_validates_minimal_contract_ref_alignment() {
        let signature = ProcedureSignature {
            name: QualifiedName::parse("Inventory.ReserveStock").unwrap(),
            accepts: Vec::new(),
            returns: vec![ResultContract {
                name: "Reservation".to_string(),
                cardinality: Cardinality::One,
                columns: vec![ColumnDescriptor {
                    name: "Reserved".to_string(),
                    data_type: TypeDescriptor::required(ScalarType::Bool),
                    ordinal: 0,
                }],
            }],
        };
        let contract_ref = ProcedureContractRef {
            procedure_id: ProcedureId::new(7),
            contract_hash: ContractHash::test_vector(0xAA),
            catalog_version: CatalogVersion::new(3),
        };

        assert!(signature
            .validate_against_contract_ref(&contract_ref)
            .is_ok());

        let invalid_ref = ProcedureContractRef {
            contract_hash: ContractHash::zero(),
            ..contract_ref
        };

        assert_eq!(
            signature
                .validate_against_contract_ref(&invalid_ref)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Contract
        );
    }
}
