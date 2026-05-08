/// Stable identifier for the catalog diff placeholder taxonomy.
pub const CATALOG_DIFF_TAXONOMY_ID: &str = "andromeda.catalog_diff.v0";

/// Placeholder schema version for future catalog diff ownership.
pub const CATALOG_DIFF_SCHEMA_VERSION: u16 = 0;

pub const CATALOG_DIFF_KIND_OBJECT_ADDED: &str = "kind.object_added";
pub const CATALOG_DIFF_KIND_OBJECT_REMOVED: &str = "kind.object_removed";
pub const CATALOG_DIFF_KIND_OBJECT_REPLACED: &str = "kind.object_replaced";

pub const CATALOG_DIFF_IMPACT_CONTRACT_HASH_CHANGED: &str = "impact.contract_hash_changed";
pub const CATALOG_DIFF_IMPACT_DEPENDENCY_CHANGED: &str = "impact.dependency_changed";
pub const CATALOG_DIFF_IMPACT_PERMISSION_CHANGED: &str = "impact.permission_changed";

pub const CATALOG_DIFF_SEVERITY_INFORMATIONAL: &str = "severity.informational";
pub const CATALOG_DIFF_SEVERITY_WAL_REQUIRED: &str = "severity.wal_required";
pub const CATALOG_DIFF_SEVERITY_BREAKING_REVIEW: &str = "severity.breaking_review";

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

pub type CatalogDiffKind = TaxonomyEntry;
pub type CatalogDiffImpactKind = TaxonomyEntry;
pub type CatalogDiffSeverity = TaxonomyEntry;

pub const ALL_CATALOG_DIFF_KINDS: &[CatalogDiffKind] = &[
    CatalogDiffKind {
        id: CATALOG_DIFF_KIND_OBJECT_ADDED,
        label: "Catalog object added",
        status: TaxonomyStatus::Reserved,
    },
    CatalogDiffKind {
        id: CATALOG_DIFF_KIND_OBJECT_REMOVED,
        label: "Catalog object removed",
        status: TaxonomyStatus::Reserved,
    },
    CatalogDiffKind {
        id: CATALOG_DIFF_KIND_OBJECT_REPLACED,
        label: "Catalog object replaced",
        status: TaxonomyStatus::Reserved,
    },
];

pub const ALL_CATALOG_DIFF_IMPACT_KINDS: &[CatalogDiffImpactKind] = &[
    CatalogDiffImpactKind {
        id: CATALOG_DIFF_IMPACT_CONTRACT_HASH_CHANGED,
        label: "Contract hash changed",
        status: TaxonomyStatus::Reserved,
    },
    CatalogDiffImpactKind {
        id: CATALOG_DIFF_IMPACT_DEPENDENCY_CHANGED,
        label: "Dependency graph changed",
        status: TaxonomyStatus::Reserved,
    },
    CatalogDiffImpactKind {
        id: CATALOG_DIFF_IMPACT_PERMISSION_CHANGED,
        label: "Permission contract changed",
        status: TaxonomyStatus::Reserved,
    },
];

pub const ALL_CATALOG_DIFF_SEVERITIES: &[CatalogDiffSeverity] = &[
    CatalogDiffSeverity {
        id: CATALOG_DIFF_SEVERITY_INFORMATIONAL,
        label: "Informational diff",
        status: TaxonomyStatus::Reserved,
    },
    CatalogDiffSeverity {
        id: CATALOG_DIFF_SEVERITY_WAL_REQUIRED,
        label: "Durable WAL required before visible effect",
        status: TaxonomyStatus::Reserved,
    },
    CatalogDiffSeverity {
        id: CATALOG_DIFF_SEVERITY_BREAKING_REVIEW,
        label: "Breaking change requires review",
        status: TaxonomyStatus::Reserved,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_taxonomy_has_wal_severity() {
        assert!(
            ALL_CATALOG_DIFF_SEVERITIES
                .iter()
                .any(|entry| entry.id == CATALOG_DIFF_SEVERITY_WAL_REQUIRED)
        );
    }

    #[test]
    fn taxonomy_id_is_reserved_v0() {
        assert_eq!(CATALOG_DIFF_TAXONOMY_ID, "andromeda.catalog_diff.v0");
        assert_eq!(CATALOG_DIFF_SCHEMA_VERSION, 0);
    }
}
