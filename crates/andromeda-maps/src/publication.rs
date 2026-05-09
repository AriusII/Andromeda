use crate::{MapDescriptor, MapDescriptorError, MapDescriptorResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapPublicationCandidate {
    pub descriptor: MapDescriptor,
    pub catalog_version: u64,
    pub stats_version: u64,
}

impl MapPublicationCandidate {
    pub const fn new(
        descriptor: MapDescriptor,
        catalog_version: u64,
        stats_version: u64,
    ) -> MapDescriptorResult<Self> {
        if catalog_version == 0 {
            return Err(MapDescriptorError::ZeroCatalogVersion);
        }
        if stats_version == 0 {
            return Err(MapDescriptorError::ZeroStatsVersion);
        }

        Ok(Self {
            descriptor,
            catalog_version,
            stats_version,
        })
    }

    pub fn publish(
        self,
        publication_lsn: u64,
        validation_digest: [u8; 32],
    ) -> MapDescriptorResult<MapPublicationEvidence> {
        MapPublicationEvidence::new(
            self.descriptor,
            self.catalog_version,
            self.stats_version,
            publication_lsn,
            validation_digest,
        )
    }

    pub fn validate(
        self,
        validation_digest: [u8; 32],
    ) -> MapDescriptorResult<MapValidatedPublicationCandidate> {
        MapValidatedPublicationCandidate::new(
            self.descriptor,
            self.catalog_version,
            self.stats_version,
            validation_digest,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapValidatedPublicationCandidate {
    pub descriptor: MapDescriptor,
    pub catalog_version: u64,
    pub stats_version: u64,
    pub validation_digest: [u8; 32],
}

impl MapValidatedPublicationCandidate {
    pub const fn new(
        descriptor: MapDescriptor,
        catalog_version: u64,
        stats_version: u64,
        validation_digest: [u8; 32],
    ) -> MapDescriptorResult<Self> {
        if catalog_version == 0 {
            return Err(MapDescriptorError::ZeroCatalogVersion);
        }
        if stats_version == 0 {
            return Err(MapDescriptorError::ZeroStatsVersion);
        }
        if digest_is_empty(&validation_digest) {
            return Err(MapDescriptorError::EmptyValidationDigest);
        }

        Ok(Self {
            descriptor,
            catalog_version,
            stats_version,
            validation_digest,
        })
    }

    pub fn publish(self, publication_lsn: u64) -> MapDescriptorResult<MapPublicationEvidence> {
        MapPublicationEvidence::new(
            self.descriptor,
            self.catalog_version,
            self.stats_version,
            publication_lsn,
            self.validation_digest,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapPublicationEvidence {
    pub descriptor: MapDescriptor,
    pub catalog_version: u64,
    pub stats_version: u64,
    pub publication_lsn: u64,
    pub validation_digest: [u8; 32],
}

impl MapPublicationEvidence {
    pub const fn new(
        descriptor: MapDescriptor,
        catalog_version: u64,
        stats_version: u64,
        publication_lsn: u64,
        validation_digest: [u8; 32],
    ) -> MapDescriptorResult<Self> {
        if catalog_version == 0 {
            return Err(MapDescriptorError::ZeroCatalogVersion);
        }
        if stats_version == 0 {
            return Err(MapDescriptorError::ZeroStatsVersion);
        }
        if publication_lsn == 0 {
            return Err(MapDescriptorError::ZeroPublicationLsn);
        }
        if digest_is_empty(&validation_digest) {
            return Err(MapDescriptorError::EmptyValidationDigest);
        }

        Ok(Self {
            descriptor,
            catalog_version,
            stats_version,
            publication_lsn,
            validation_digest,
        })
    }

    pub const fn is_current_for(self, catalog_version: u64, stats_version: u64) -> bool {
        self.catalog_version == catalog_version && self.stats_version == stats_version
    }

    pub const fn is_rebuildable_projection(self) -> bool {
        true
    }

    pub const fn is_source_truth(self) -> bool {
        false
    }

    pub const fn rebuild(
        self,
        rebuild_lsn: u64,
        rebuild_digest: [u8; 32],
    ) -> MapDescriptorResult<MapPublicationRebuildEvidence> {
        MapPublicationRebuildEvidence::new(self, rebuild_lsn, rebuild_digest)
    }

    pub const fn recover(
        self,
        recovery_lsn: u64,
        recovery_digest: [u8; 32],
    ) -> MapDescriptorResult<MapPublicationRecoveryEvidence> {
        MapPublicationRecoveryEvidence::new(self, recovery_lsn, recovery_digest)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapPublicationSwitch {
    pub previous_active: Option<MapPublicationEvidence>,
    pub active: MapPublicationEvidence,
}

impl MapPublicationSwitch {
    pub const fn new(
        previous_active: Option<MapPublicationEvidence>,
        active: MapPublicationEvidence,
    ) -> Self {
        Self {
            previous_active,
            active,
        }
    }

    pub fn from_candidate(
        previous_active: Option<MapPublicationEvidence>,
        candidate: MapPublicationCandidate,
        publication_lsn: u64,
        validation_digest: [u8; 32],
    ) -> MapDescriptorResult<Self> {
        let validated = candidate.validate(validation_digest)?;

        Self::from_validated(previous_active, validated, publication_lsn)
    }

    pub fn from_validated(
        previous_active: Option<MapPublicationEvidence>,
        validated: MapValidatedPublicationCandidate,
        publication_lsn: u64,
    ) -> MapDescriptorResult<Self> {
        let active = validated.publish(publication_lsn)?;

        Ok(Self::new(previous_active, active))
    }

    pub const fn active_state(self) -> MapPublicationState {
        MapPublicationState::Active(self.active)
    }

    pub const fn rollback(
        self,
        rollback_lsn: u64,
        rollback_digest: [u8; 32],
    ) -> MapDescriptorResult<MapPublicationRollbackEvidence> {
        MapPublicationRollbackEvidence::new(
            self.active,
            self.previous_active,
            rollback_lsn,
            rollback_digest,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapPublicationRollbackEvidence {
    pub rolled_back_active: MapPublicationEvidence,
    pub restored_active: Option<MapPublicationEvidence>,
    pub rollback_lsn: u64,
    pub rollback_digest: [u8; 32],
}

impl MapPublicationRollbackEvidence {
    pub const fn new(
        rolled_back_active: MapPublicationEvidence,
        restored_active: Option<MapPublicationEvidence>,
        rollback_lsn: u64,
        rollback_digest: [u8; 32],
    ) -> MapDescriptorResult<Self> {
        if rollback_lsn == 0 {
            return Err(MapDescriptorError::ZeroRollbackLsn);
        }
        if digest_is_empty(&rollback_digest) {
            return Err(MapDescriptorError::EmptyRollbackDigest);
        }

        Ok(Self {
            rolled_back_active,
            restored_active,
            rollback_lsn,
            rollback_digest,
        })
    }

    pub const fn active_after_rollback(self) -> Option<MapPublicationEvidence> {
        self.restored_active
    }

    pub const fn is_rebuildable_projection(self) -> bool {
        true
    }

    pub const fn is_source_truth(self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapPublicationRebuildEvidence {
    pub active: MapPublicationEvidence,
    pub rebuild_lsn: u64,
    pub rebuild_digest: [u8; 32],
}

impl MapPublicationRebuildEvidence {
    pub const fn new(
        active: MapPublicationEvidence,
        rebuild_lsn: u64,
        rebuild_digest: [u8; 32],
    ) -> MapDescriptorResult<Self> {
        if rebuild_lsn == 0 {
            return Err(MapDescriptorError::ZeroRebuildLsn);
        }
        if digest_is_empty(&rebuild_digest) {
            return Err(MapDescriptorError::EmptyRebuildDigest);
        }

        Ok(Self {
            active,
            rebuild_lsn,
            rebuild_digest,
        })
    }

    pub const fn candidate(self) -> MapPublicationCandidate {
        MapPublicationCandidate {
            descriptor: self.active.descriptor,
            catalog_version: self.active.catalog_version,
            stats_version: self.active.stats_version,
        }
    }

    pub const fn is_rebuildable_projection(self) -> bool {
        true
    }

    pub const fn is_source_truth(self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapPublicationRecoveryEvidence {
    pub active: MapPublicationEvidence,
    pub recovery_lsn: u64,
    pub recovery_digest: [u8; 32],
}

impl MapPublicationRecoveryEvidence {
    pub const fn new(
        active: MapPublicationEvidence,
        recovery_lsn: u64,
        recovery_digest: [u8; 32],
    ) -> MapDescriptorResult<Self> {
        if recovery_lsn == 0 {
            return Err(MapDescriptorError::ZeroRecoveryLsn);
        }
        if digest_is_empty(&recovery_digest) {
            return Err(MapDescriptorError::EmptyRecoveryDigest);
        }

        Ok(Self {
            active,
            recovery_lsn,
            recovery_digest,
        })
    }

    pub const fn active_after_recovery(self) -> MapPublicationEvidence {
        self.active
    }

    pub const fn is_rebuildable_projection(self) -> bool {
        true
    }

    pub const fn is_source_truth(self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapPublicationState {
    Candidate(MapPublicationCandidate),
    Validated(MapValidatedPublicationCandidate),
    Active(MapPublicationEvidence),
    RolledBack(MapPublicationRollbackEvidence),
    RebuildRequired(MapPublicationRebuildEvidence),
    Recovered(MapPublicationRecoveryEvidence),
}

impl MapPublicationState {
    pub const fn active(self) -> Option<MapPublicationEvidence> {
        match self {
            Self::Active(active) => Some(active),
            Self::Recovered(recovery) => Some(recovery.active),
            Self::RolledBack(rollback) => rollback.restored_active,
            Self::Candidate(_) | Self::Validated(_) | Self::RebuildRequired(_) => None,
        }
    }

    pub const fn is_active_visible(self) -> bool {
        self.active().is_some()
    }

    pub const fn is_rebuildable_projection(self) -> bool {
        true
    }

    pub const fn is_source_truth(self) -> bool {
        false
    }
}

const fn digest_is_empty(digest: &[u8; 32]) -> bool {
    let mut index = 0;
    while index < digest.len() {
        if digest[index] != 0 {
            return false;
        }
        index += 1;
    }
    true
}
