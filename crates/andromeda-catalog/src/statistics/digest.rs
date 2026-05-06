pub(crate) const STATS_PUBLICATION_DOMAIN: &[u8] = b"andromeda.stats.publication.v0";
pub(crate) const STATS_CORRELATION_DOMAIN: &[u8] = b"andromeda.stats.correlation.v0";
pub(crate) const STATS_CORRELATION_PUBLICATION_DOMAIN: &[u8] =
    b"andromeda.stats.correlation_publication.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatsCorrelationDigest([u8; Self::LEN]);

impl StatsCorrelationDigest {
    pub const LEN: usize = 32;

    pub const fn from_bytes(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatsPublicationDigest([u8; Self::LEN]);

impl StatsPublicationDigest {
    pub const LEN: usize = 32;

    pub const fn from_bytes(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }
}
