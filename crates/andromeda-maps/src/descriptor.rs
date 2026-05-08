use crate::{MapDescriptorError, MapDescriptorResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MapId(u64);

impl MapId {
    pub fn new(value: u64) -> MapDescriptorResult<Self> {
        if value == 0 {
            return Err(MapDescriptorError::ZeroMapId);
        }

        Ok(Self(value))
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapGrain {
    Relation,
    Partition,
    Segment,
    KeyRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapRefreshMode {
    Immediate,
    Deferred,
    Scheduled,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapStalenessPolicy {
    CurrentOnly,
    BoundedMillis(u64),
    AdvisorySnapshot,
}

impl MapStalenessPolicy {
    pub fn bounded_millis(max_age_millis: u64) -> MapDescriptorResult<Self> {
        if max_age_millis == 0 {
            return Err(MapDescriptorError::ZeroBoundedStalenessMillis);
        }

        Ok(Self::BoundedMillis(max_age_millis))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapDescriptor {
    pub id: MapId,
    pub grain: MapGrain,
    pub refresh_mode: MapRefreshMode,
    pub staleness: MapStalenessPolicy,
}

impl MapDescriptor {
    pub const fn new(
        id: MapId,
        grain: MapGrain,
        refresh_mode: MapRefreshMode,
        staleness: MapStalenessPolicy,
    ) -> Self {
        Self {
            id,
            grain,
            refresh_mode,
            staleness,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_id_rejects_zero() {
        assert_eq!(MapId::new(0).unwrap_err(), MapDescriptorError::ZeroMapId);
        assert_eq!(MapId::new(7).unwrap().get(), 7);
    }

    #[test]
    fn bounded_staleness_rejects_zero_duration() {
        assert_eq!(
            MapStalenessPolicy::bounded_millis(0).unwrap_err(),
            MapDescriptorError::ZeroBoundedStalenessMillis
        );
        assert_eq!(
            MapStalenessPolicy::bounded_millis(250).unwrap(),
            MapStalenessPolicy::BoundedMillis(250)
        );
    }

    #[test]
    fn descriptor_carries_only_declared_map_metadata() {
        let descriptor = MapDescriptor::new(
            MapId::new(9).unwrap(),
            MapGrain::Relation,
            MapRefreshMode::Deferred,
            MapStalenessPolicy::AdvisorySnapshot,
        );

        assert_eq!(descriptor.id.get(), 9);
        assert_eq!(descriptor.grain, MapGrain::Relation);
        assert_eq!(descriptor.refresh_mode, MapRefreshMode::Deferred);
        assert_eq!(descriptor.staleness, MapStalenessPolicy::AdvisorySnapshot);
    }
}
