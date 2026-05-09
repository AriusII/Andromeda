#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataTemperature {
    RamWorkingSet,
    Hot,
    Cooling,
    Cold,
}

impl DataTemperature {
    pub const fn preferred_tier(self) -> StorageTier {
        match self {
            Self::RamWorkingSet => StorageTier::Ram,
            Self::Hot | Self::Cooling => StorageTier::HotStore,
            Self::Cold => StorageTier::ColdStore,
        }
    }

    pub const fn is_cold(self) -> bool {
        matches!(self, Self::Cold)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageTier {
    Ram,
    HotStore,
    ColdStore,
}

impl StorageTier {
    pub const fn media_class(self) -> &'static str {
        match self {
            Self::Ram => "volatile RAM working set",
            Self::HotStore => "NVMe/SSD HotStore",
            Self::ColdStore => "HDD ColdStore",
        }
    }

    pub const fn is_mutable(self) -> bool {
        !matches!(self, Self::ColdStore)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStage {
    BufferInRam,
    AppendHotStore,
    SealHotStoreSegment,
    PublishColdStore,
    ServeRead,
}

impl PipelineStage {
    pub const fn source_tier(self) -> Option<StorageTier> {
        match self {
            Self::BufferInRam => None,
            Self::AppendHotStore => Some(StorageTier::Ram),
            Self::SealHotStoreSegment => Some(StorageTier::HotStore),
            Self::PublishColdStore => Some(StorageTier::HotStore),
            Self::ServeRead => None,
        }
    }

    pub const fn destination_tier(self) -> Option<StorageTier> {
        match self {
            Self::BufferInRam => Some(StorageTier::Ram),
            Self::AppendHotStore | Self::SealHotStoreSegment => Some(StorageTier::HotStore),
            Self::PublishColdStore => Some(StorageTier::ColdStore),
            Self::ServeRead => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadFallbackPolicy {
    RamThenHotThenCold,
    HotThenCold,
    ColdOnly,
}

impl ReadFallbackPolicy {
    pub const fn for_temperature(temperature: DataTemperature) -> Self {
        match temperature {
            DataTemperature::RamWorkingSet => Self::RamThenHotThenCold,
            DataTemperature::Hot | DataTemperature::Cooling => Self::HotThenCold,
            DataTemperature::Cold => Self::ColdOnly,
        }
    }

    pub const fn allows_tier(self, tier: StorageTier) -> bool {
        match self {
            Self::RamThenHotThenCold => true,
            Self::HotThenCold => matches!(tier, StorageTier::HotStore | StorageTier::ColdStore),
            Self::ColdOnly => matches!(tier, StorageTier::ColdStore),
        }
    }
}
