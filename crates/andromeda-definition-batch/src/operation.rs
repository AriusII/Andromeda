use andromeda_catalog_store::CatalogLifecycleMutationTarget;
use andromeda_contract::{CatalogDefinition, CatalogObjectRef, ObjectKind, QualifiedName};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogObjectId, CatalogVersion};

/// A single operation within a DefinitionBatch.
#[allow(
    clippy::large_enum_variant,
    reason = "DefinitionBatch keeps catalog definitions inline for stable public contract shape."
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefinitionOperation {
    Create(CatalogDefinition),
    Deprecate(CatalogLifecycleTarget),
}

/// Identifies the catalog object targeted by a lifecycle transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogLifecycleTarget {
    pub object: CatalogObjectRef,
}

impl CatalogLifecycleTarget {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.object.validate_for_definition(self.object.kind)
    }
}

impl CatalogLifecycleMutationTarget for CatalogLifecycleTarget {
    fn validate_for_catalog_mutation(&self) -> AndromedaResult<()> {
        self.validate()
    }

    fn object_ref(&self) -> &CatalogObjectRef {
        &self.object
    }
}

/// A successfully planned object creation within a DefinitionBatch dry-run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedDefinition {
    pub object_id: CatalogObjectId,
    pub name: QualifiedName,
    pub kind: ObjectKind,
    pub planned_version: CatalogVersion,
}

/// The lifecycle action applied to an existing catalog object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogLifecycleAction {
    Deprecate,
}

/// A successfully planned lifecycle transition within a DefinitionBatch dry-run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedLifecycleTransition {
    pub object_id: CatalogObjectId,
    pub name: QualifiedName,
    pub kind: ObjectKind,
    pub action: CatalogLifecycleAction,
    pub planned_version: CatalogVersion,
}

pub(crate) fn object_kind_tag(kind: ObjectKind) -> u8 {
    match kind {
        ObjectKind::Database => 0,
        ObjectKind::Namespace => 1,
        ObjectKind::Table => 2,
        ObjectKind::Map => 3,
        ObjectKind::Enum => 4,
        ObjectKind::StructuredObject => 5,
        ObjectKind::Procedure => 6,
    }
}

pub(crate) fn duplicate_lifecycle_error() -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Catalog,
        "definition batch must not change the same lifecycle object id twice",
    )
}

pub(crate) fn duplicate_lifecycle_name_error() -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Catalog,
        "definition batch must not change the same lifecycle object name twice",
    )
}
