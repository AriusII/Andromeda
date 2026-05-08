//! Runtime-free catalog object diff model.
//!
//! This module computes typed object-level evidence only. It does not evaluate
//! Procedure compatibility, apply catalog changes, publish snapshots, or bypass
//! durable WAL requirements.

use andromeda_catalog_store::{CatalogDefinition, CatalogObjectRef};
use andromeda_types::ContractHash;

/// Object-level catalog diff kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogObjectDiffKind {
    Added,
    Removed,
    Replaced,
}

/// Object-level impact detected by diff evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogObjectDiffImpact {
    LifecycleChanged,
    ObjectIdentityChanged,
    QualifiedNameChanged,
    ObjectKindChanged,
    CatalogVersionChanged,
    ContractHashChanged,
}

/// Review severity for object-level diff evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogObjectDiffSeverity {
    Informational,
    WalRequired,
    BreakingReview,
}

/// Typed evidence for one catalog object-level diff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogObjectDiff {
    pub kind: CatalogObjectDiffKind,
    pub previous: Option<CatalogObjectRef>,
    pub next: Option<CatalogObjectRef>,
    pub previous_shape_hash: Option<ContractHash>,
    pub next_shape_hash: Option<ContractHash>,
    pub impacts: Vec<CatalogObjectDiffImpact>,
    pub severity: CatalogObjectDiffSeverity,
}

impl CatalogObjectDiff {
    pub fn between(
        previous: Option<&CatalogDefinition>,
        next: Option<&CatalogDefinition>,
    ) -> Option<Self> {
        diff_catalog_object_definitions(previous, next)
    }

    pub fn requires_durable_wal(&self) -> bool {
        self.severity != CatalogObjectDiffSeverity::Informational
    }
}

/// Diffs two optional catalog object definitions.
///
/// `None` plus `None` and identical definitions produce no diff evidence.
pub fn diff_catalog_object_definitions(
    previous: Option<&CatalogDefinition>,
    next: Option<&CatalogDefinition>,
) -> Option<CatalogObjectDiff> {
    match (previous, next) {
        (None, None) => None,
        (None, Some(next)) => Some(added_object_diff(next)),
        (Some(previous), None) => Some(removed_object_diff(previous)),
        (Some(previous), Some(next)) if previous == next => None,
        (Some(previous), Some(next)) => Some(replaced_object_diff(previous, next)),
    }
}

fn added_object_diff(next: &CatalogDefinition) -> CatalogObjectDiff {
    CatalogObjectDiff {
        kind: CatalogObjectDiffKind::Added,
        previous: None,
        next: Some(next.object_ref().clone()),
        previous_shape_hash: None,
        next_shape_hash: Some(next.shape_hash()),
        impacts: vec![CatalogObjectDiffImpact::LifecycleChanged],
        severity: CatalogObjectDiffSeverity::WalRequired,
    }
}

fn removed_object_diff(previous: &CatalogDefinition) -> CatalogObjectDiff {
    CatalogObjectDiff {
        kind: CatalogObjectDiffKind::Removed,
        previous: Some(previous.object_ref().clone()),
        next: None,
        previous_shape_hash: Some(previous.shape_hash()),
        next_shape_hash: None,
        impacts: vec![CatalogObjectDiffImpact::LifecycleChanged],
        severity: CatalogObjectDiffSeverity::WalRequired,
    }
}

fn replaced_object_diff(
    previous: &CatalogDefinition,
    next: &CatalogDefinition,
) -> CatalogObjectDiff {
    let previous_ref = previous.object_ref();
    let next_ref = next.object_ref();
    let previous_shape_hash = previous.shape_hash();
    let next_shape_hash = next.shape_hash();
    let mut impacts = Vec::new();

    if previous_ref.object_id != next_ref.object_id {
        impacts.push(CatalogObjectDiffImpact::ObjectIdentityChanged);
    }
    if previous_ref.name != next_ref.name {
        impacts.push(CatalogObjectDiffImpact::QualifiedNameChanged);
    }
    if previous_ref.kind != next_ref.kind {
        impacts.push(CatalogObjectDiffImpact::ObjectKindChanged);
    }
    if previous_ref.catalog_version != next_ref.catalog_version {
        impacts.push(CatalogObjectDiffImpact::CatalogVersionChanged);
    }
    if previous_shape_hash != next_shape_hash {
        impacts.push(CatalogObjectDiffImpact::ContractHashChanged);
    }

    let severity = if impacts.iter().any(|impact| {
        matches!(
            impact,
            CatalogObjectDiffImpact::ObjectIdentityChanged
                | CatalogObjectDiffImpact::QualifiedNameChanged
                | CatalogObjectDiffImpact::ObjectKindChanged
        )
    }) {
        CatalogObjectDiffSeverity::BreakingReview
    } else if impacts.is_empty() {
        CatalogObjectDiffSeverity::Informational
    } else {
        CatalogObjectDiffSeverity::WalRequired
    };

    CatalogObjectDiff {
        kind: CatalogObjectDiffKind::Replaced,
        previous: Some(previous_ref.clone()),
        next: Some(next_ref.clone()),
        previous_shape_hash: Some(previous_shape_hash),
        next_shape_hash: Some(next_shape_hash),
        impacts,
        severity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_catalog_store::{ObjectKind, QualifiedName, TableDefinition};
    use andromeda_types::{
        CatalogObjectId, CatalogVersion, ColumnDescriptor, ScalarType, TypeDescriptor,
    };

    fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
        ColumnDescriptor {
            name: name.to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal,
        }
    }

    fn table(version: u64, columns: Vec<ColumnDescriptor>) -> CatalogDefinition {
        CatalogDefinition::Table(TableDefinition {
            object: CatalogObjectRef {
                object_id: CatalogObjectId::new(1),
                name: QualifiedName::parse("Inventory.Product").unwrap(),
                kind: ObjectKind::Table,
                catalog_version: CatalogVersion::new(version),
            },
            columns,
        })
    }

    #[test]
    fn no_diff_for_identical_definition() {
        let definition = table(3, vec![column("ProductId", 0)]);

        assert_eq!(
            diff_catalog_object_definitions(Some(&definition), Some(&definition)),
            None
        );
    }

    #[test]
    fn added_object_requires_wal_before_visibility() {
        let next = table(4, vec![column("ProductId", 0)]);
        let diff = diff_catalog_object_definitions(None, Some(&next)).unwrap();

        assert_eq!(diff.kind, CatalogObjectDiffKind::Added);
        assert_eq!(diff.severity, CatalogObjectDiffSeverity::WalRequired);
        assert!(diff.requires_durable_wal());
    }

    #[test]
    fn replacement_tracks_version_and_shape_hash_impacts() {
        let previous = table(3, vec![column("ProductId", 0)]);
        let next = table(
            4,
            vec![column("ProductId", 0), column("QuantityAvailable", 1)],
        );

        let diff = CatalogObjectDiff::between(Some(&previous), Some(&next)).unwrap();

        assert_eq!(diff.kind, CatalogObjectDiffKind::Replaced);
        assert_eq!(diff.severity, CatalogObjectDiffSeverity::WalRequired);
        assert!(
            diff.impacts
                .contains(&CatalogObjectDiffImpact::CatalogVersionChanged)
        );
        assert!(
            diff.impacts
                .contains(&CatalogObjectDiffImpact::ContractHashChanged)
        );
        assert_ne!(diff.previous_shape_hash, diff.next_shape_hash);
    }
}
