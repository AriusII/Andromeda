/// Stable identifier for the contract compatibility placeholder taxonomy.
pub const CONTRACT_COMPAT_TAXONOMY_ID: &str = "andromeda.contract.compat.v0";

/// Placeholder schema version for future compatibility ownership.
pub const CONTRACT_COMPAT_SCHEMA_VERSION: u16 = 0;

pub const CONTRACT_COMPAT_COMPATIBLE_EXACT: &str = "compatible.exact";
pub const CONTRACT_COMPAT_COMPATIBLE_ADDITIVE: &str = "compatible.additive";
pub const CONTRACT_COMPAT_COMPATIBLE_METADATA_ONLY: &str = "compatible.metadata_only";
pub const CONTRACT_COMPAT_INCOMPATIBLE_SHAPE: &str = "incompatible.shape";
pub const CONTRACT_COMPAT_INCOMPATIBLE_PERMISSION: &str = "incompatible.permission";
pub const CONTRACT_COMPAT_REVIEW_REQUIRED: &str = "review.required";

/// Stability state for taxonomy entries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaxonomyStatus {
    /// The identifier is reserved for future implementation work.
    Reserved,
    /// The identifier is actively consumed by a production code path
    /// (e.g. `CompatibilityDecision::taxonomy_id()`).
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

/// Contract compatibility placeholder class.
pub type ContractCompatibilityClass = TaxonomyEntry;

pub const ALL_CONTRACT_COMPATIBILITY_CLASSES: &[ContractCompatibilityClass] = &[
    ContractCompatibilityClass {
        id: CONTRACT_COMPAT_COMPATIBLE_EXACT,
        label: "Exact contract match",
        status: TaxonomyStatus::Reserved,
    },
    ContractCompatibilityClass {
        id: CONTRACT_COMPAT_COMPATIBLE_ADDITIVE,
        label: "Additive contract change",
        status: TaxonomyStatus::Active,
    },
    ContractCompatibilityClass {
        id: CONTRACT_COMPAT_COMPATIBLE_METADATA_ONLY,
        label: "Metadata-only contract change",
        status: TaxonomyStatus::Active,
    },
    ContractCompatibilityClass {
        id: CONTRACT_COMPAT_INCOMPATIBLE_SHAPE,
        label: "Input or output shape incompatibility",
        status: TaxonomyStatus::Active,
    },
    ContractCompatibilityClass {
        id: CONTRACT_COMPAT_INCOMPATIBLE_PERMISSION,
        label: "Permission contract incompatibility",
        status: TaxonomyStatus::Active,
    },
    ContractCompatibilityClass {
        id: CONTRACT_COMPAT_REVIEW_REQUIRED,
        label: "Compatibility requires explicit review",
        status: TaxonomyStatus::Active,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taxonomy_ids_are_unique() {
        for (index, entry) in ALL_CONTRACT_COMPATIBILITY_CLASSES.iter().enumerate() {
            assert!(!entry.id.is_empty());
            assert!(
                ALL_CONTRACT_COMPATIBILITY_CLASSES[..index]
                    .iter()
                    .all(|seen| seen.id != entry.id)
            );
        }
    }

    #[test]
    fn taxonomy_id_is_reserved_v0() {
        assert_eq!(CONTRACT_COMPAT_TAXONOMY_ID, "andromeda.contract.compat.v0");
        assert_eq!(CONTRACT_COMPAT_SCHEMA_VERSION, 0);
    }

    #[test]
    fn taxonomy_active_entries_are_consumed_by_compatibility_decision() {
        // Five entries are now Active — consumed by CompatibilityDecision::taxonomy_id().
        // compatible.exact remains Reserved (not directly mapped to a CompatibilityDecision variant).
        let active_ids = [
            CONTRACT_COMPAT_COMPATIBLE_ADDITIVE,
            CONTRACT_COMPAT_COMPATIBLE_METADATA_ONLY,
            CONTRACT_COMPAT_INCOMPATIBLE_SHAPE,
            CONTRACT_COMPAT_INCOMPATIBLE_PERMISSION,
            CONTRACT_COMPAT_REVIEW_REQUIRED,
        ];
        for id in active_ids {
            let entry = ALL_CONTRACT_COMPATIBILITY_CLASSES
                .iter()
                .find(|e| e.id == id)
                .unwrap_or_else(|| panic!("missing taxonomy entry for {id}"));
            assert_eq!(
                entry.status,
                TaxonomyStatus::Active,
                "taxonomy entry {id} must be Active once consumed by CompatibilityDecision"
            );
        }
        // compatible.exact must remain Reserved.
        let exact = ALL_CONTRACT_COMPATIBILITY_CLASSES
            .iter()
            .find(|e| e.id == CONTRACT_COMPAT_COMPATIBLE_EXACT)
            .unwrap();
        assert_eq!(exact.status, TaxonomyStatus::Reserved);
    }
}
