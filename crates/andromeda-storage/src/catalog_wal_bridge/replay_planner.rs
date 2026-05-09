use andromeda_catalog_recovery::{
    CatalogMutationRecordKind, CatalogWalObservedRecord, CatalogWalPublicationRecordMetadata,
    replay_catalog_publications_from_observed_wal,
};
use andromeda_core::{AndromedaResult, CatalogVersion};

use crate::wal_record_catalog::CatalogWalRecord;
use crate::{WalRecord, WalRecordKind};

use super::codec::decode_catalog_record;
use super::error::{catalog_wal_error, lsn_order_error};

pub type CatalogWalPublicationRecord =
    andromeda_catalog_recovery::CatalogWalPublicationRecord<CatalogWalRecord>;
pub type CatalogWalDurablePublication =
    andromeda_catalog_recovery::CatalogWalDurablePublication<CatalogWalRecord>;
pub type CatalogWalPublicationReplayReport =
    andromeda_catalog_recovery::CatalogWalPublicationReplayReport<CatalogWalRecord>;

impl CatalogWalPublicationRecordMetadata for CatalogWalRecord {
    fn catalog_version(&self) -> Option<CatalogVersion> {
        self.catalog_version()
    }
}

/// Replays catalog publication boundaries from storage WAL records.
///
/// Storage is now only the adapter from concrete `WalRecord` values to the
/// recovery-owned runtime-free replay planner.
pub fn replay_catalog_publications_from_wal(
    records: &[WalRecord],
) -> AndromedaResult<CatalogWalPublicationReplayReport> {
    validate_wal_lsn_order(records)?;

    let observed_records = records
        .iter()
        .filter_map(observed_catalog_record)
        .collect::<AndromedaResult<Vec<_>>>()?;

    replay_catalog_publications_from_observed_wal(
        records.len(),
        observed_records,
        decode_apply_payload,
    )
}

fn validate_wal_lsn_order(records: &[WalRecord]) -> AndromedaResult<()> {
    let mut last_lsn = None;
    for record in records {
        if let Some(previous_lsn) = last_lsn
            && record.header.lsn <= previous_lsn
        {
            return Err(lsn_order_error(previous_lsn, record.header.lsn));
        }
        last_lsn = Some(record.header.lsn);
    }
    Ok(())
}

fn observed_catalog_record(
    record: &WalRecord,
) -> Option<AndromedaResult<CatalogWalObservedRecord<'_>>> {
    let kind = match record.header.kind {
        WalRecordKind::CatalogChangeBegin => CatalogMutationRecordKind::CatalogChangeBegin,
        WalRecordKind::CatalogChangeApply => CatalogMutationRecordKind::CatalogChangeApply,
        WalRecordKind::CatalogChangeCommit => CatalogMutationRecordKind::CatalogChangeCommit,
        _ => return None,
    };

    Some(record.validate().map(|()| CatalogWalObservedRecord {
        lsn: record.header.lsn,
        kind,
        transaction_id: record.header.transaction_id,
        payload: record.payload(),
    }))
}

fn decode_apply_payload(record: CatalogWalObservedRecord<'_>) -> AndromedaResult<CatalogWalRecord> {
    decode_catalog_record(record.payload).map_err(|err| {
        catalog_wal_error(format!(
            "catalog WAL apply payload at LSN {} is not a valid catalog record: {}",
            record.lsn.get(),
            err.message()
        ))
    })
}
