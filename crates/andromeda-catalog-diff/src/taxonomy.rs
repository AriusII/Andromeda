/// Stable identifier for the catalog diff taxonomy.
pub const CATALOG_DIFF_TAXONOMY_ID: &str = "andromeda.catalog_diff.v0";

/// Schema version for object-level catalog diff evidence.
pub const CATALOG_DIFF_SCHEMA_VERSION: u16 = 1;

pub const CATALOG_DIFF_KIND_OBJECT_ADDED: &str = "kind.object_added";
pub const CATALOG_DIFF_KIND_OBJECT_REMOVED: &str = "kind.object_removed";
pub const CATALOG_DIFF_KIND_OBJECT_REPLACED: &str = "kind.object_replaced";

pub const CATALOG_DIFF_IMPACT_CONTRACT_HASH_CHANGED: &str = "impact.contract_hash_changed";
pub const CATALOG_DIFF_IMPACT_DEPENDENCY_CHANGED: &str = "impact.dependency_changed";
pub const CATALOG_DIFF_IMPACT_PERMISSION_CHANGED: &str = "impact.permission_changed";

pub const CATALOG_DIFF_SEVERITY_INFORMATIONAL: &str = "severity.informational";
pub const CATALOG_DIFF_SEVERITY_WAL_REQUIRED: &str = "severity.wal_required";
pub const CATALOG_DIFF_SEVERITY_BREAKING_REVIEW: &str = "severity.breaking_review";

/// Stability state for taxonomy entries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaxonomyStatus {
    /// The identifier is owned by `andromeda-catalog-diff`.
    Active,
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
        status: TaxonomyStatus::Active,
    },
    CatalogDiffKind {
        id: CATALOG_DIFF_KIND_OBJECT_REMOVED,
        label: "Catalog object removed",
        status: TaxonomyStatus::Active,
    },
    CatalogDiffKind {
        id: CATALOG_DIFF_KIND_OBJECT_REPLACED,
        label: "Catalog object replaced",
        status: TaxonomyStatus::Active,
    },
];

pub const ALL_CATALOG_DIFF_IMPACT_KINDS: &[CatalogDiffImpactKind] = &[
    CatalogDiffImpactKind {
        id: CATALOG_DIFF_IMPACT_CONTRACT_HASH_CHANGED,
        label: "Contract hash changed",
        status: TaxonomyStatus::Active,
    },
    CatalogDiffImpactKind {
        id: CATALOG_DIFF_IMPACT_DEPENDENCY_CHANGED,
        label: "Dependency graph changed",
        status: TaxonomyStatus::Active,
    },
    CatalogDiffImpactKind {
        id: CATALOG_DIFF_IMPACT_PERMISSION_CHANGED,
        label: "Permission contract changed",
        status: TaxonomyStatus::Active,
    },
];

pub const ALL_CATALOG_DIFF_SEVERITIES: &[CatalogDiffSeverity] = &[
    CatalogDiffSeverity {
        id: CATALOG_DIFF_SEVERITY_INFORMATIONAL,
        label: "Informational diff",
        status: TaxonomyStatus::Active,
    },
    CatalogDiffSeverity {
        id: CATALOG_DIFF_SEVERITY_WAL_REQUIRED,
        label: "Durable WAL required before visible effect",
        status: TaxonomyStatus::Active,
    },
    CatalogDiffSeverity {
        id: CATALOG_DIFF_SEVERITY_BREAKING_REVIEW,
        label: "Breaking change requires review",
        status: TaxonomyStatus::Active,
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
    fn taxonomy_id_is_active_v1() {
        assert_eq!(CATALOG_DIFF_TAXONOMY_ID, "andromeda.catalog_diff.v0");
        assert_eq!(CATALOG_DIFF_SCHEMA_VERSION, 1);
        assert!(
            ALL_CATALOG_DIFF_KINDS
                .iter()
                .all(|entry| entry.status == TaxonomyStatus::Active)
        );
    }
}
