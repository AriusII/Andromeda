//! Structural catalog dependencies derived from contract-safe definitions.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{
    CatalogBindingKind, CatalogDefinition, CatalogObjectBinding, ObjectKind, QualifiedName,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CatalogDependencyKind {
    /// A namespace contains a catalog object (object namespace membership).
    /// The dependent is a `Namespace`; the dependency can be any catalog
    /// object kind (`Table`, `Procedure`, `StructuredObject`, `Enum`, `Map`,
    /// or `Database`).
    ///
    /// Defined by `SPEC_CATALOG_OBJECT_MODEL_V0.md` §Dependency edge kinds.
    NamespaceContainsObject,
    /// A procedure consumes a structured object as one of its structured
    /// inputs. Derived directly from `ProcedureContract::structured_inputs`.
    ProcedureStructuredInput,
    /// A procedure reads from a table at execution time. Derived from
    /// catalog object bindings (`CatalogBindingKind::ReadsTable`) supplied
    /// alongside the definition batch.
    ProcedureReadsTable,
    /// A procedure writes to a table at execution time. Derived from
    /// catalog object bindings (`CatalogBindingKind::WritesTable`).
    ProcedureWritesTable,
    /// A procedure emits a structured-object result as one of its result
    /// streams. Derived from `CatalogBindingKind::EmitsStructuredObject`.
    ProcedureEmitsStructuredObject,
}

impl CatalogDependencyKind {
    pub fn dependent_kind(self) -> ObjectKind {
        match self {
            Self::NamespaceContainsObject => ObjectKind::Namespace,
            Self::ProcedureStructuredInput
            | Self::ProcedureReadsTable
            | Self::ProcedureWritesTable
            | Self::ProcedureEmitsStructuredObject => ObjectKind::Procedure,
        }
    }

    /// Returns the expected dependency object kind for kinds where the
    /// dependency kind is fixed. Returns `None` for `NamespaceContainsObject`,
    /// where any catalog object kind is valid.
    pub fn fixed_dependency_kind(self) -> Option<ObjectKind> {
        match self {
            Self::NamespaceContainsObject => None,
            Self::ProcedureStructuredInput | Self::ProcedureEmitsStructuredObject => {
                Some(ObjectKind::StructuredObject)
            },
            Self::ProcedureReadsTable | Self::ProcedureWritesTable => Some(ObjectKind::Table),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogDependency {
    pub kind: CatalogDependencyKind,
    pub dependent_name: QualifiedName,
    pub dependent_kind: ObjectKind,
    pub dependency_name: QualifiedName,
    pub dependency_kind: ObjectKind,
}

impl CatalogDependency {
    pub fn procedure_structured_input(
        dependent_name: QualifiedName,
        dependency_name: QualifiedName,
    ) -> Self {
        Self::with_kind(
            CatalogDependencyKind::ProcedureStructuredInput,
            dependent_name,
            dependency_name,
        )
    }

    pub fn procedure_reads_table(
        dependent_name: QualifiedName,
        dependency_name: QualifiedName,
    ) -> Self {
        Self::with_kind(
            CatalogDependencyKind::ProcedureReadsTable,
            dependent_name,
            dependency_name,
        )
    }

    pub fn procedure_writes_table(
        dependent_name: QualifiedName,
        dependency_name: QualifiedName,
    ) -> Self {
        Self::with_kind(
            CatalogDependencyKind::ProcedureWritesTable,
            dependent_name,
            dependency_name,
        )
    }

    pub fn procedure_emits_structured_object(
        dependent_name: QualifiedName,
        dependency_name: QualifiedName,
    ) -> Self {
        Self::with_kind(
            CatalogDependencyKind::ProcedureEmitsStructuredObject,
            dependent_name,
            dependency_name,
        )
    }

    /// Construct a `NamespaceContainsObject` edge.
    ///
    /// `namespace_name` is the containing namespace (dependent).
    /// `object_name` is the contained catalog object (dependency).
    /// `object_kind` is the object kind of the contained object; it can be
    /// any valid `ObjectKind` other than `Namespace`.
    ///
    /// This constructor is used instead of [`Self::with_kind`] because the
    /// dependency kind is variable across instances of this edge kind.
    pub fn namespace_contains_object(
        namespace_name: QualifiedName,
        object_name: QualifiedName,
        object_kind: ObjectKind,
    ) -> Self {
        Self {
            kind: CatalogDependencyKind::NamespaceContainsObject,
            dependent_name: namespace_name,
            dependent_kind: ObjectKind::Namespace,
            dependency_name: object_name,
            dependency_kind: object_kind,
        }
    }

    fn with_kind(
        kind: CatalogDependencyKind,
        dependent_name: QualifiedName,
        dependency_name: QualifiedName,
    ) -> Self {
        let dependency_kind = kind
            .fixed_dependency_kind()
            .expect("with_kind requires a kind with a fixed dependency kind");
        Self {
            kind,
            dependent_name,
            dependent_kind: kind.dependent_kind(),
            dependency_name,
            dependency_kind,
        }
    }

    /// Lower a `CatalogObjectBinding` into a structural catalog dependency
    /// edge. The caller remains responsible for validating the binding.
    pub fn from_binding(binding: &CatalogObjectBinding) -> Self {
        let kind = match binding.kind {
            CatalogBindingKind::ReadsTable => CatalogDependencyKind::ProcedureReadsTable,
            CatalogBindingKind::WritesTable => CatalogDependencyKind::ProcedureWritesTable,
            CatalogBindingKind::UsesStructuredInput => {
                CatalogDependencyKind::ProcedureStructuredInput
            },
            CatalogBindingKind::EmitsStructuredObject => {
                CatalogDependencyKind::ProcedureEmitsStructuredObject
            },
        };
        Self::with_kind(
            kind,
            binding.dependent.name.clone(),
            binding.dependency.name.clone(),
        )
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.dependent_kind != self.kind.dependent_kind() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog dependency dependent kind must match dependency kind",
            ));
        }

        // For edge kinds with a fixed dependency kind, validate that the
        // stored dependency kind matches the expected kind. For
        // `NamespaceContainsObject`, the dependency kind is variable (any
        // catalog object kind is valid), so no fixed-kind check is performed.
        if let Some(expected_dependency_kind) = self.kind.fixed_dependency_kind()
            && self.dependency_kind != expected_dependency_kind
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog dependency dependency kind must match dependency kind",
            ));
        }

        if self.dependent_name == self.dependency_name {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog dependency dependent and dependency names must differ",
            ));
        }

        Ok(())
    }
}

impl CatalogDefinition {
    pub fn dependencies(&self) -> Vec<CatalogDependency> {
        match self {
            Self::Procedure(procedure) => procedure
                .structured_inputs
                .iter()
                .map(|structured_input| {
                    CatalogDependency::procedure_structured_input(
                        procedure.object.name.clone(),
                        structured_input.clone(),
                    )
                })
                .collect(),
            Self::Table(_) | Self::StructuredObject(_) | Self::Enum(_) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CatalogObjectRef, ProcedureContract, QualifiedName, StatsVersion, TransactionPolicy,
    };
    use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash, ProcedureId};

    fn object(id: u64, name: &str, kind: ObjectKind, version: u64) -> CatalogObjectRef {
        CatalogObjectRef {
            object_id: CatalogObjectId::new(id),
            name: QualifiedName::parse(name).unwrap(),
            kind,
            catalog_version: CatalogVersion::new(version),
        }
    }

    fn procedure(structured_inputs: Vec<QualifiedName>) -> ProcedureContract {
        ProcedureContract {
            object: object(1, "Inventory.ReserveStock", ObjectKind::Procedure, 7),
            procedure_id: ProcedureId::new(1),
            contract_hash: ContractHash::test_vector(0xA1),
            stats_version: StatsVersion::new(1),
            protocol_layout: crate::ProtocolLayoutRef {
                descriptor_set_hash: ContractHash::test_vector(0xA2),
                frame_envelope_hash: ContractHash::test_vector(0xA3),
            },
            inputs: Vec::new(),
            structured_inputs,
            result_streams: Vec::new(),
            required_permissions: vec!["Inventory.ReserveStock.Execute".to_string()],
            transaction_policy: TransactionPolicy {
                access_mode: crate::AccessMode::ReadWrite,
                isolation: crate::IsolationPolicy::Serializable,
                retryable: false,
            },
            compatibility_policy: crate::CompatibilityPolicy::ExactHash,
            result_metadata_policy: crate::ResultMetadataPolicy::RequireBeforePayload,
            error_policy: crate::ProcedureErrorPolicy {
                rollback_on_error: true,
                allowed_error_codes: Vec::new(),
            },
            multi_result_policy: crate::MultiResultPolicy::SingleResultOnly,
        }
    }

    #[test]
    fn procedure_structured_inputs_are_contract_dependencies() {
        let dependency_name = QualifiedName::parse("Inventory.StockRequest").unwrap();
        let definition = CatalogDefinition::Procedure(procedure(vec![dependency_name.clone()]));
        let dependencies = definition.dependencies();

        assert_eq!(dependencies.len(), 1);
        assert_eq!(
            dependencies[0].kind,
            CatalogDependencyKind::ProcedureStructuredInput
        );
        assert_eq!(dependencies[0].dependent_kind, ObjectKind::Procedure);
        assert_eq!(
            dependencies[0].dependency_kind,
            ObjectKind::StructuredObject
        );
        assert_eq!(dependencies[0].dependency_name, dependency_name);
    }
}
