use andromeda_error::AndromedaResult;

use crate::Lsn;

use super::error::storage_error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColdExtentReclaimEvidence {
    pub retired_manifest_version: u64,
    pub reclaim_lsn: Lsn,
    pub catalog_epoch: u64,
}

impl ColdExtentReclaimEvidence {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.retired_manifest_version == 0 || self.catalog_epoch == 0 {
            return Err(storage_error(
                "cold extent reclaim evidence must carry nonzero manifest and catalog anchors",
            ));
        }
        if self.reclaim_lsn.is_zero() {
            return Err(storage_error("cold extent reclaim LSN must not be zero"));
        }
        Ok(())
    }
}
