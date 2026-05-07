use andromeda_types::CatalogVersion;

use crate::{contracts::StatsVersion, digest::Sha256};

use super::super::{
    HistogramPlaceholder, STATS_PUBLICATION_DOMAIN, StatsColumnTarget, StatsPublicationDigest,
};

pub(super) fn compute_publication_digest(
    catalog_version: CatalogVersion,
    version: StatsVersion,
    entries: &[(StatsColumnTarget, HistogramPlaceholder)],
) -> StatsPublicationDigest {
    let mut hasher = Sha256::new();
    hasher.update(STATS_PUBLICATION_DOMAIN);
    hasher.update(&[0xD0]);
    hasher.update(&catalog_version.get().to_le_bytes());
    hasher.update(&[0xD1]);
    hasher.update(&version.get().to_le_bytes());
    hasher.update(&[0xD2]);
    hasher.update(&(entries.len() as u32).to_le_bytes());
    for (target, histogram) in entries {
        hasher.update(&[0xD3]);
        hasher.update(&target.object_id.get().to_le_bytes());
        hasher.update(&target.column_index.to_le_bytes());
        histogram.absorb(&mut hasher);
    }

    StatsPublicationDigest::from_bytes(hasher.finalize())
}
