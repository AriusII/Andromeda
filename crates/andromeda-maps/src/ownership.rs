use crate::{
    descriptor::MapDescriptor,
    publication::{
        MapPublicationCandidate, MapPublicationEvidence, MapPublicationState,
        MapValidatedPublicationCandidate,
    },
    summarizability::MapSummarizabilityPolicy,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapEvidenceAuthority {
    DescriptorOnly,
    DurableOwnerSupplied,
}

pub trait MapOwnershipBoundary {
    fn evidence_authority(&self) -> MapEvidenceAuthority;

    fn owns_source_truth(&self) -> bool {
        false
    }

    fn owns_durable_publication_path(&self) -> bool {
        false
    }
}

impl MapOwnershipBoundary for MapDescriptor {
    fn evidence_authority(&self) -> MapEvidenceAuthority {
        MapEvidenceAuthority::DescriptorOnly
    }
}

impl MapOwnershipBoundary for MapSummarizabilityPolicy {
    fn evidence_authority(&self) -> MapEvidenceAuthority {
        MapEvidenceAuthority::DescriptorOnly
    }
}

impl MapOwnershipBoundary for MapPublicationCandidate {
    fn evidence_authority(&self) -> MapEvidenceAuthority {
        MapEvidenceAuthority::DescriptorOnly
    }
}

impl MapOwnershipBoundary for MapValidatedPublicationCandidate {
    fn evidence_authority(&self) -> MapEvidenceAuthority {
        MapEvidenceAuthority::DescriptorOnly
    }
}

impl MapOwnershipBoundary for MapPublicationEvidence {
    fn evidence_authority(&self) -> MapEvidenceAuthority {
        MapEvidenceAuthority::DurableOwnerSupplied
    }
}

impl MapOwnershipBoundary for MapPublicationState {
    fn evidence_authority(&self) -> MapEvidenceAuthority {
        match self {
            Self::Candidate(_) | Self::Validated(_) => MapEvidenceAuthority::DescriptorOnly,
            Self::Active(_)
            | Self::RolledBack(_)
            | Self::RebuildRequired(_)
            | Self::Recovered(_) => MapEvidenceAuthority::DurableOwnerSupplied,
        }
    }
}
