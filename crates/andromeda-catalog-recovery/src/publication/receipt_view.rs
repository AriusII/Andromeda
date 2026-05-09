use andromeda_definition_batch::{
    DefinitionBatchDependencyGraphHash, DefinitionBatchId, DefinitionBatchSourceHash,
};
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

use crate::CatalogPublicationSemantics;

pub trait CatalogPublicationReceiptView {
    type DurabilityMarker: Clone + PartialEq + Eq;

    fn batch_id_value(&self) -> u64;
    fn database_id(&self) -> DatabaseId;
    fn namespace_id(&self) -> NamespaceId;
    fn previous_version(&self) -> CatalogVersion;
    fn next_version(&self) -> CatalogVersion;
    fn durable_lsn(&self) -> Option<u64>;
    fn durable_evidence_marker(&self) -> Option<&Self::DurabilityMarker>;
    fn record_count(&self) -> usize;
    fn source_hash_is_zero(&self) -> bool;
    fn dependency_graph_hash_is_zero(&self) -> bool;
    fn publication_semantics(&self) -> CatalogPublicationSemantics;
}

pub trait CatalogPublicationBatchIdView {
    fn batch_id_value(&self) -> u64;
}

impl CatalogPublicationBatchIdView for DefinitionBatchId {
    fn batch_id_value(&self) -> u64 {
        self.get()
    }
}

pub trait CatalogPublicationHashEvidence {
    fn is_zero(&self) -> bool;
}

impl CatalogPublicationHashEvidence for DefinitionBatchSourceHash {
    fn is_zero(&self) -> bool {
        (*self).is_zero()
    }
}

impl CatalogPublicationHashEvidence for DefinitionBatchDependencyGraphHash {
    fn is_zero(&self) -> bool {
        (*self).is_zero()
    }
}

impl<BatchId, SourceHash, DependencyGraphHash> CatalogPublicationReceiptView
    for andromeda_catalog_store::CatalogPublicationReceipt<BatchId, SourceHash, DependencyGraphHash>
where
    BatchId: CatalogPublicationBatchIdView + Copy,
    SourceHash: CatalogPublicationHashEvidence + Copy,
    DependencyGraphHash: CatalogPublicationHashEvidence + Copy,
{
    type DurabilityMarker = andromeda_catalog_store::CatalogDurabilityMarker;

    fn batch_id_value(&self) -> u64 {
        CatalogPublicationBatchIdView::batch_id_value(&self.batch_id)
    }

    fn database_id(&self) -> DatabaseId {
        self.database_id
    }

    fn namespace_id(&self) -> NamespaceId {
        self.namespace_id
    }

    fn previous_version(&self) -> CatalogVersion {
        self.previous_version
    }

    fn next_version(&self) -> CatalogVersion {
        self.next_version
    }

    fn durable_lsn(&self) -> Option<u64> {
        self.durable_lsn
    }

    fn durable_evidence_marker(&self) -> Option<&Self::DurabilityMarker> {
        self.durable_evidence_marker.as_ref()
    }

    fn record_count(&self) -> usize {
        self.record_count
    }

    fn source_hash_is_zero(&self) -> bool {
        self.source_hash.is_zero()
    }

    fn dependency_graph_hash_is_zero(&self) -> bool {
        self.dependency_graph_hash.is_zero()
    }

    fn publication_semantics(&self) -> CatalogPublicationSemantics {
        match self.publication_semantics {
            andromeda_catalog_store::CatalogPublicationSemantics::PlannedVersionOnly => {
                CatalogPublicationSemantics::PlannedVersionOnly
            },
            andromeda_catalog_store::CatalogPublicationSemantics::DurablePublicationExternal => {
                CatalogPublicationSemantics::DurablePublicationExternal
            },
        }
    }
}
