use andromeda_catalog::CatalogWalRecord;
use andromeda_types::CatalogVersion;

use super::fixtures::apply_catalog_version_record;
use andromeda_catalog::DefinitionBatchId;

#[test]
fn batch_wal_correlation_preserves_identities() {
    let batch_id = DefinitionBatchId::new(777);
    let version = CatalogVersion::new(88);

    let apply_record = apply_catalog_version_record(batch_id.get(), version.get(), 2, 5000);

    let CatalogWalRecord::ApplyCatalogVersion {
        batch_id: recorded_batch_id,
        version: recorded_version,
        ..
    } = apply_record
    else {
        panic!("Expected ApplyCatalogVersion");
    };

    assert_eq!(
        recorded_batch_id, batch_id,
        "Batch ID must be preserved for correlation"
    );
    assert_eq!(
        recorded_version, version,
        "Version must be preserved for correlation"
    );
}

#[test]
fn batch_wal_correlation_lsn_monotonic_check() {
    let batch1 = apply_catalog_version_record(1, 1, 1, 1000);
    let batch2 = apply_catalog_version_record(2, 2, 1, 2000);

    assert!(batch1.validate().is_ok());
    assert!(batch2.validate().is_ok());

    let (
        CatalogWalRecord::ApplyCatalogVersion { lsn: lsn1, .. },
        CatalogWalRecord::ApplyCatalogVersion { lsn: lsn2, .. },
    ) = (&batch1, &batch2)
    else {
        panic!("Expected ApplyCatalogVersion records");
    };

    assert!(
        lsn1 < lsn2,
        "LSN must be monotonically increasing for correlation"
    );
}
