use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_procedure_contract::{ProcedureContractRef, QualifiedName};
use andromeda_types::ColumnDescriptor;
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
                return Err(srpl_error("SRPL result stream names must be unique"));
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
            return Err(srpl_error("SRPL result contract name must not be empty"));
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
        return Err(srpl_error(
            "SRPL result contract must declare at least one column",
        ));
    }

    let mut column_names = BTreeSet::new();
    for (expected_ordinal, column) in columns.iter().enumerate() {
        column.validate()?;
        if !column_names.insert(column.name.as_str()) {
            return Err(srpl_error(format!("{context} names must be unique")));
        }
        if column.ordinal != expected_ordinal as u32 {
            return Err(srpl_error(format!(
                "{context} must be dense and zero-based"
            )));
        }
    }

    Ok(())
}

fn srpl_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Srpl, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_types::{
        CatalogVersion, ContractHash, ProcedureId, ScalarType, TimestampType, TypeDescriptor,
    };

    fn column(name: &str, scalar_type: ScalarType, ordinal: u32) -> ColumnDescriptor {
        ColumnDescriptor {
            name: name.to_string(),
            data_type: TypeDescriptor::required(scalar_type),
            ordinal,
        }
    }

    fn result_contract(name: &str, columns: Vec<ColumnDescriptor>) -> ResultContract {
        ResultContract {
            name: name.to_string(),
            cardinality: Cardinality::One,
            columns,
        }
    }

    fn signature(
        accepts: Vec<ColumnDescriptor>,
        returns: Vec<ResultContract>,
    ) -> ProcedureSignature {
        ProcedureSignature {
            name: QualifiedName::parse("Inventory.ReserveStock").unwrap(),
            accepts,
            returns,
        }
    }

    #[test]
    fn procedure_signature_validates_contract_shapes() {
        let signature = signature(
            vec![column("ProductId", ScalarType::I64, 0)],
            vec![result_contract(
                "Reservation",
                vec![column(
                    "ReservedAt",
                    ScalarType::Timestamp(TimestampType::Transaction),
                    0,
                )],
            )],
        );

        assert!(signature.validate().is_ok());
    }

    #[test]
    fn procedure_signature_rejects_sparse_parameter_ordinals() {
        let signature = signature(
            vec![column("ProductId", ScalarType::I64, 1)],
            vec![result_contract(
                "Reservation",
                vec![column("Reserved", ScalarType::Bool, 0)],
            )],
        );

        let error = signature.validate().unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
        assert!(error.message().contains("dense"));
    }

    #[test]
    fn result_contract_rejects_empty_or_sparse_columns() {
        let empty = result_contract("Reservation", Vec::new());

        assert_eq!(
            empty.validate().unwrap_err().kind(),
            AndromedaErrorKind::Srpl
        );

        let sparse = result_contract("Reservation", vec![column("Reserved", ScalarType::Bool, 2)]);

        let error = sparse.validate().unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
        assert!(error.message().contains("dense"));
    }

    #[test]
    fn procedure_signature_rejects_duplicate_parameter_names() {
        let signature = signature(
            vec![
                column("ProductId", ScalarType::I64, 0),
                column("ProductId", ScalarType::I64, 1),
            ],
            vec![result_contract(
                "Reservation",
                vec![column("Reserved", ScalarType::Bool, 0)],
            )],
        );

        let error = signature.validate().unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
        assert!(error.message().contains("unique"));
    }

    #[test]
    fn procedure_signature_validates_minimal_contract_ref_alignment() {
        let signature = signature(
            Vec::new(),
            vec![result_contract(
                "Reservation",
                vec![column("Reserved", ScalarType::Bool, 0)],
            )],
        );
        let contract_ref = ProcedureContractRef {
            procedure_id: ProcedureId::new(7),
            contract_hash: ContractHash::test_vector(0xAA),
            catalog_version: CatalogVersion::new(3),
        };

        assert!(
            signature
                .validate_against_contract_ref(&contract_ref)
                .is_ok()
        );

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
