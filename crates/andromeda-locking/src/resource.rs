use andromeda_error::AndromedaResult;

use super::{LockMode, validate_non_zero};

/// Storage-agnostic resource identity used by the lock manager.
///
/// The identifiers are intentionally opaque `u64` values. Zero is rejected by
/// constructors because it is reserved across Andromeda identifier domains as
/// "no id" / "no resource".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LockResource {
    Schema {
        schema_id: u64,
    },
    Object {
        schema_id: u64,
        object_id: u64,
    },
    Table {
        schema_id: u64,
        table_id: u64,
    },
    Page {
        schema_id: u64,
        table_id: u64,
        page_id: u64,
    },
    Row {
        schema_id: u64,
        table_id: u64,
        row_id: u64,
    },
}

impl LockResource {
    pub fn schema(schema_id: u64) -> AndromedaResult<Self> {
        validate_non_zero(schema_id, "lock schema resource id must not be zero")?;
        Ok(Self::Schema { schema_id })
    }

    pub fn table(schema_id: u64, table_id: u64) -> AndromedaResult<Self> {
        validate_non_zero(schema_id, "lock table schema id must not be zero")?;
        validate_non_zero(table_id, "lock table resource id must not be zero")?;
        Ok(Self::Table {
            schema_id,
            table_id,
        })
    }

    pub fn object(schema_id: u64, object_id: u64) -> AndromedaResult<Self> {
        validate_non_zero(schema_id, "lock object schema id must not be zero")?;
        validate_non_zero(object_id, "lock object resource id must not be zero")?;
        Ok(Self::Object {
            schema_id,
            object_id,
        })
    }

    pub fn page(schema_id: u64, table_id: u64, page_id: u64) -> AndromedaResult<Self> {
        validate_non_zero(schema_id, "lock page schema id must not be zero")?;
        validate_non_zero(table_id, "lock page table id must not be zero")?;
        validate_non_zero(page_id, "lock page resource id must not be zero")?;
        Ok(Self::Page {
            schema_id,
            table_id,
            page_id,
        })
    }

    pub fn row(schema_id: u64, table_id: u64, row_id: u64) -> AndromedaResult<Self> {
        validate_non_zero(schema_id, "lock row schema id must not be zero")?;
        validate_non_zero(table_id, "lock row table id must not be zero")?;
        validate_non_zero(row_id, "lock row resource id must not be zero")?;
        Ok(Self::Row {
            schema_id,
            table_id,
            row_id,
        })
    }
}

/// A typed lock acquisition requested as part of a multi-resource protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LockRequest {
    pub resource: LockResource,
    pub mode: LockMode,
}

impl LockRequest {
    pub const fn new(resource: LockResource, mode: LockMode) -> Self {
        Self { resource, mode }
    }
}

/// Build schema/object intention lock plans for catalog-safe operations.
///
/// These helpers deliberately use only opaque transaction resource ids and do
/// not depend on `andromeda-catalog`. Callers that know catalog identities map
/// `NamespaceId`/schema ids and catalog object ids to the opaque `u64` values at
/// the boundary.
pub struct CatalogIntentionLocks;

impl CatalogIntentionLocks {
    /// Procedure invocation / contract use: hold intent-shared on the schema and
    /// schema-shared stability on the object being invoked or referenced.
    ///
    /// This blocks a concurrent DefinitionBatch mutation that requests
    /// [`LockMode::SchemaExclusive`] on the same object while remaining
    /// compatible with data intent work (`IS`/`IX`) on the schema.
    pub fn procedure_invocation(
        schema_id: u64,
        object_id: u64,
    ) -> AndromedaResult<[LockRequest; 2]> {
        Ok([
            LockRequest::new(LockResource::schema(schema_id)?, LockMode::IntentShared),
            LockRequest::new(
                LockResource::object(schema_id, object_id)?,
                LockMode::SchemaShared,
            ),
        ])
    }

    /// DefinitionBatch object mutation: hold intent-exclusive on the schema and
    /// schema-exclusive on each changed object.
    ///
    /// The schema intent allows multiple disjoint object-level DefinitionBatch
    /// operations to queue according to the lock manager's FIFO rules without
    /// over-locking the full schema. Whole-schema DDL should request
    /// [`LockMode::SchemaExclusive`] on [`LockResource::Schema`], which conflicts
    /// with both invocation `IS` and mutation `IX` ancestors.
    pub fn definition_batch_object_mutation(
        schema_id: u64,
        object_id: u64,
    ) -> AndromedaResult<[LockRequest; 2]> {
        Ok([
            LockRequest::new(LockResource::schema(schema_id)?, LockMode::IntentExclusive),
            LockRequest::new(
                LockResource::object(schema_id, object_id)?,
                LockMode::SchemaExclusive,
            ),
        ])
    }

    /// Whole-schema catalog mutation: request schema-exclusive at the schema
    /// root. This conflicts with invocation and object-mutation ancestors.
    pub fn definition_batch_schema_mutation(schema_id: u64) -> AndromedaResult<[LockRequest; 1]> {
        Ok([LockRequest::new(
            LockResource::schema(schema_id)?,
            LockMode::SchemaExclusive,
        )])
    }
}

pub(super) fn validate_lock_resource(resource: LockResource) -> AndromedaResult<()> {
    match resource {
        LockResource::Schema { schema_id } => {
            validate_non_zero(schema_id, "lock schema resource id must not be zero")?;
        },
        LockResource::Object {
            schema_id,
            object_id,
        } => {
            validate_non_zero(schema_id, "lock object schema id must not be zero")?;
            validate_non_zero(object_id, "lock object resource id must not be zero")?;
        },
        LockResource::Table {
            schema_id,
            table_id,
        } => {
            validate_non_zero(schema_id, "lock table schema id must not be zero")?;
            validate_non_zero(table_id, "lock table resource id must not be zero")?;
        },
        LockResource::Page {
            schema_id,
            table_id,
            page_id,
        } => {
            validate_non_zero(schema_id, "lock page schema id must not be zero")?;
            validate_non_zero(table_id, "lock page table id must not be zero")?;
            validate_non_zero(page_id, "lock page resource id must not be zero")?;
        },
        LockResource::Row {
            schema_id,
            table_id,
            row_id,
        } => {
            validate_non_zero(schema_id, "lock row schema id must not be zero")?;
            validate_non_zero(table_id, "lock row table id must not be zero")?;
            validate_non_zero(row_id, "lock row resource id must not be zero")?;
        },
    }

    Ok(())
}
