/// Stable identifier for the catalog store placeholder taxonomy.
pub const CATALOG_STORE_TAXONOMY_ID: &str = "andromeda.catalog_store.v0";

/// Placeholder schema version for future catalog store ownership.
pub const CATALOG_STORE_SCHEMA_VERSION: u16 = 0;

pub const CATALOG_STORE_RESPONSIBILITY_ROOT_POINTER: &str = "responsibility.root_pointer";
pub const CATALOG_STORE_RESPONSIBILITY_OBJECT_LOOKUP: &str = "responsibility.object_lookup";
pub const CATALOG_STORE_RESPONSIBILITY_VERSION_HISTORY: &str = "responsibility.version_history";

pub const CATALOG_STORE_SNAPSHOT_VISIBLE_CATALOG: &str = "snapshot.visible_catalog";
pub const CATALOG_STORE_SNAPSHOT_CONSISTENT_READ: &str = "snapshot.consistent_read";
pub const CATALOG_STORE_SNAPSHOT_FORENSIC_READ: &str = "snapshot.forensic_read";

pub const CATALOG_STORE_BARRIER_DURABLE_WAL_REQUIRED: &str = "barrier.durable_wal_required";
pub const CATALOG_STORE_BARRIER_AUDIT_TRACE_REQUIRED: &str = "barrier.audit_trace_required";
pub const CATALOG_STORE_BARRIER_RECOVERY_VALIDATED: &str = "barrier.recovery_validated";

/// Stability state for placeholder taxonomy entries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaxonomyStatus {
    /// The identifier is reserved for future implementation work.
    Reserved,
}

/// Runtime-free taxonomy entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaxonomyEntry {
    /// Stable dotted identifier.
    pub id: &'static str,
    /// Human-readable label for planning and documentation.
    pub label: &'static str,
    /// Placeholder stability state.
    pub status: TaxonomyStatus,
}

pub type CatalogStoreResponsibility = TaxonomyEntry;
pub type CatalogStoreSnapshotClass = TaxonomyEntry;
pub type CatalogStoreBarrier = TaxonomyEntry;

pub const ALL_CATALOG_STORE_RESPONSIBILITIES: &[CatalogStoreResponsibility] = &[
    CatalogStoreResponsibility {
        id: CATALOG_STORE_RESPONSIBILITY_ROOT_POINTER,
        label: "Catalog root pointer ownership",
        status: TaxonomyStatus::Reserved,
    },
    CatalogStoreResponsibility {
        id: CATALOG_STORE_RESPONSIBILITY_OBJECT_LOOKUP,
        label: "Catalog object lookup ownership",
        status: TaxonomyStatus::Reserved,
    },
    CatalogStoreResponsibility {
        id: CATALOG_STORE_RESPONSIBILITY_VERSION_HISTORY,
        label: "Catalog version history ownership",
        status: TaxonomyStatus::Reserved,
    },
];

pub const ALL_CATALOG_STORE_SNAPSHOT_CLASSES: &[CatalogStoreSnapshotClass] = &[
    CatalogStoreSnapshotClass {
        id: CATALOG_STORE_SNAPSHOT_VISIBLE_CATALOG,
        label: "Visible catalog snapshot",
        status: TaxonomyStatus::Reserved,
    },
    CatalogStoreSnapshotClass {
        id: CATALOG_STORE_SNAPSHOT_CONSISTENT_READ,
        label: "Consistent catalog read snapshot",
        status: TaxonomyStatus::Reserved,
    },
    CatalogStoreSnapshotClass {
        id: CATALOG_STORE_SNAPSHOT_FORENSIC_READ,
        label: "Forensic catalog read snapshot",
        status: TaxonomyStatus::Reserved,
    },
];

pub const ALL_CATALOG_STORE_BARRIERS: &[CatalogStoreBarrier] = &[
    CatalogStoreBarrier {
        id: CATALOG_STORE_BARRIER_DURABLE_WAL_REQUIRED,
        label: "Durable WAL required before visible publication",
        status: TaxonomyStatus::Reserved,
    },
    CatalogStoreBarrier {
        id: CATALOG_STORE_BARRIER_AUDIT_TRACE_REQUIRED,
        label: "Audit trace required for catalog publication",
        status: TaxonomyStatus::Reserved,
    },
    CatalogStoreBarrier {
        id: CATALOG_STORE_BARRIER_RECOVERY_VALIDATED,
        label: "Recovery behavior validated before critical use",
        status: TaxonomyStatus::Reserved,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_taxonomy_has_publication_barrier() {
        assert!(
            ALL_CATALOG_STORE_BARRIERS
                .iter()
                .any(|entry| entry.id == CATALOG_STORE_BARRIER_DURABLE_WAL_REQUIRED)
        );
    }

    #[test]
    fn taxonomy_id_is_reserved_v0() {
        assert_eq!(CATALOG_STORE_TAXONOMY_ID, "andromeda.catalog_store.v0");
        assert_eq!(CATALOG_STORE_SCHEMA_VERSION, 0);
    }
}
