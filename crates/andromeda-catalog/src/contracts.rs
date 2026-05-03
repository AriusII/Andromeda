use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ColumnDescriptor,
    ContractHash, ProcedureId,
};

use crate::{names::QualifiedName, objects::validate_columns, CatalogObjectRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcedureContractRef {
    pub procedure_id: ProcedureId,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
}

impl ProcedureContractRef {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.contract_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure contract hash must not be zero",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationPolicy {
    Snapshot,
    Serializable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionPolicy {
    pub access_mode: AccessMode,
    pub isolation: IsolationPolicy,
    pub retryable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatibilityPolicy {
    AdditiveOnly,
    ExactHash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultStreamContract {
    pub name: String,
    pub columns: Vec<ColumnDescriptor>,
    pub row_count_exact_required: bool,
}

impl ResultStreamContract {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.name.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream name must not be empty",
            ));
        }

        validate_columns(&self.columns)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureContract {
    pub object: CatalogObjectRef,
    pub procedure_id: ProcedureId,
    pub contract_hash: ContractHash,
    pub inputs: Vec<ColumnDescriptor>,
    pub structured_inputs: Vec<QualifiedName>,
    pub result_streams: Vec<ResultStreamContract>,
    pub required_permissions: Vec<String>,
    pub transaction_policy: TransactionPolicy,
    pub compatibility_policy: CompatibilityPolicy,
}

impl ProcedureContract {
    pub fn as_ref(&self) -> ProcedureContractRef {
        ProcedureContractRef {
            procedure_id: self.procedure_id,
            contract_hash: self.contract_hash,
            catalog_version: self.object.catalog_version,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.as_ref().validate()?;
        validate_columns(&self.inputs)?;

        if self.required_permissions.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Security,
                "procedure contract must declare required permissions",
            ));
        }

        for stream in &self.result_streams {
            stream.validate()?;
        }

        Ok(())
    }

    pub fn validated(self) -> AndromedaResult<Self> {
        self.validate()?;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CatalogObjectRef, ObjectKind, QualifiedName};
    use andromeda_core::{CatalogObjectId, ColumnDescriptor, ScalarType, TypeDescriptor};

    fn object(kind: ObjectKind) -> CatalogObjectRef {
        CatalogObjectRef {
            object_id: CatalogObjectId::new(1),
            name: QualifiedName::parse("Inventory.Product").unwrap(),
            kind,
            catalog_version: CatalogVersion::new(7),
        }
    }

    fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
        ColumnDescriptor {
            name: name.to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal,
        }
    }

    #[test]
    fn procedure_contract_requires_nonzero_hash_and_permission() {
        let contract = ProcedureContract {
            object: object(ObjectKind::Procedure),
            procedure_id: ProcedureId::new(99),
            contract_hash: ContractHash::zero(),
            inputs: vec![column("ProductId", 0)],
            structured_inputs: Vec::new(),
            result_streams: Vec::new(),
            required_permissions: vec!["ExecuteProcedure".to_string()],
            transaction_policy: TransactionPolicy {
                access_mode: AccessMode::ReadWrite,
                isolation: IsolationPolicy::Serializable,
                retryable: false,
            },
            compatibility_policy: CompatibilityPolicy::ExactHash,
        };

        assert_eq!(
            contract.validate().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );
    }
}
