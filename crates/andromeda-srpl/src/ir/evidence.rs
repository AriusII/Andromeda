use andromeda_catalog::{CatalogObjectRef, ObjectKind, ProcedureContractRef, QualifiedName};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, ContractHash};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplCatalogBindingEvidence {
    pub catalog_version: CatalogVersion,
    pub procedure_object: CatalogObjectRef,
    pub procedure_contract: ProcedureContractRef,
    /// Catalog objects referenced by the procedure body (tables read or
    /// updated, structured objects emitted as result streams). The list is
    /// ordered by the binder so the evidence is deterministic across
    /// equivalent SRPL inputs.
    pub bound_objects: Vec<SrplObjectBindingEvidence>,
}

impl SrplCatalogBindingEvidence {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.procedure_object
            .validate_for_definition(andromeda_catalog::ObjectKind::Procedure)?;
        self.procedure_contract.validate()?;

        if self.procedure_object.catalog_version != self.catalog_version
            || self.procedure_contract.catalog_version != self.catalog_version
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "SRPL executable plan evidence requires exact catalog version match",
            ));
        }

        if self.procedure_contract.contract_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "SRPL executable plan binding hashes must not be zero",
            ));
        }

        for bound in &self.bound_objects {
            bound.validate(bound.kind)?;
            if bound.object.catalog_version != self.catalog_version {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "SRPL executable plan evidence requires exact catalog version match",
                ));
            }
        }

        Ok(())
    }

    /// Returns evidence for the first bound object whose qualified name matches.
    pub fn find_bound_object(&self, name: &QualifiedName) -> Option<&SrplObjectBindingEvidence> {
        self.bound_objects
            .iter()
            .find(|bound| &bound.object.name == name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplObjectBindingEvidence {
    pub object: CatalogObjectRef,
    pub shape_hash: ContractHash,
    pub kind: ObjectKind,
}

impl SrplObjectBindingEvidence {
    pub fn validate(&self, expected_kind: ObjectKind) -> AndromedaResult<()> {
        self.object.validate_for_definition(expected_kind)?;
        if self.kind != expected_kind {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "SRPL object binding kind must match the bound catalog object",
            ));
        }
        if self.shape_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "SRPL object binding shape hash must not be zero",
            ));
        }
        Ok(())
    }
}
