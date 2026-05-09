use andromeda_catalog_recovery::{AlterCompatibilityPolicy, CatalogWalRecord};
use andromeda_types::{CatalogVersion, ContractHash};

use super::fixtures::{
    alter_procedure_record, apply_catalog_version_record, assert_valid_record, checkpoint_record,
    create_procedure_record, deprecate_procedure_record, drop_failure_reason,
    drop_procedure_record, object_id, test_hash,
};

#[test]
fn wal_record_roundtrip_create_procedure() {
    let record = create_procedure_record(
        1001,
        "my_procedure",
        test_hash(0xAA),
        vec![object_id(2001), object_id(2002)],
    );

    assert_valid_record(&record, "CreateProcedure record should validate");

    let CatalogWalRecord::CreateProcedure {
        procedure_id,
        name,
        contract_hash,
        dependencies,
    } = record
    else {
        panic!("Expected CreateProcedure variant");
    };

    assert_eq!(procedure_id, object_id(1001));
    assert_eq!(name, "my_procedure");
    assert_eq!(contract_hash, test_hash(0xAA));
    assert_eq!(dependencies, vec![object_id(2001), object_id(2002)]);
}

#[test]
fn wal_record_roundtrip_alter_procedure() {
    let record = alter_procedure_record(
        1001,
        test_hash(0x01),
        test_hash(0x02),
        AlterCompatibilityPolicy::AdditiveOnly,
    );

    assert_valid_record(&record, "AlterProcedure record should validate");

    let CatalogWalRecord::AlterProcedure {
        procedure_id,
        old_contract_hash,
        new_contract_hash,
        compatibility,
    } = record
    else {
        panic!("Expected AlterProcedure variant");
    };

    assert_eq!(procedure_id, object_id(1001));
    assert_eq!(old_contract_hash, test_hash(0x01));
    assert_eq!(new_contract_hash, test_hash(0x02));
    assert_eq!(compatibility, AlterCompatibilityPolicy::AdditiveOnly);
}

#[test]
fn wal_record_roundtrip_deprecate_procedure() {
    let record = deprecate_procedure_record(1001, "replaced by v2.0");

    assert_valid_record(&record, "DeprecateProcedure record should validate");

    let CatalogWalRecord::DeprecateProcedure {
        procedure_id,
        reason,
    } = record
    else {
        panic!("Expected DeprecateProcedure variant");
    };

    assert_eq!(procedure_id, object_id(1001));
    assert_eq!(reason, "replaced by v2.0");
}

#[test]
fn wal_record_roundtrip_drop_procedure_success() {
    let record = drop_procedure_record(1001, None);

    assert_valid_record(&record, "DropProcedure record should validate");

    let CatalogWalRecord::DropProcedure {
        procedure_id,
        restrict_failure_reason,
    } = record
    else {
        panic!("Expected DropProcedure variant");
    };

    assert_eq!(procedure_id, object_id(1001));
    assert!(restrict_failure_reason.is_none());
}

#[test]
fn wal_record_roundtrip_drop_procedure_restricted() {
    let record = drop_procedure_record(
        1001,
        Some(drop_failure_reason(
            2,
            "procedures p_a and p_b depend on this procedure",
        )),
    );

    assert_valid_record(&record, "DropProcedure with restriction should validate");

    let CatalogWalRecord::DropProcedure {
        procedure_id,
        restrict_failure_reason,
    } = record
    else {
        panic!("Expected DropProcedure variant");
    };

    let reason = restrict_failure_reason.expect("restricted drop must keep blocker reason");
    assert_eq!(procedure_id, object_id(1001));
    assert_eq!(reason.blocker_count, 2);
    assert!(reason.description.contains("p_a"));
}

#[test]
fn wal_record_roundtrip_apply_catalog_version() {
    let record = apply_catalog_version_record(100, 42, 3, 99999);

    assert_valid_record(&record, "ApplyCatalogVersion record should validate");

    let CatalogWalRecord::ApplyCatalogVersion {
        batch_id,
        version,
        record_count,
        lsn,
    } = record
    else {
        panic!("Expected ApplyCatalogVersion variant");
    };

    assert_eq!(batch_id.get(), 100);
    assert_eq!(version, CatalogVersion::new(42));
    assert_eq!(record_count, 3);
    assert_eq!(lsn, 99999);
}

#[test]
fn wal_record_roundtrip_checkpoint() {
    let record = checkpoint_record(7000, 77, 9);

    assert_valid_record(&record, "CatalogCheckpoint record should validate");

    let CatalogWalRecord::CatalogCheckpoint {
        checkpoint_lsn,
        catalog_version,
        visible_procedure_count,
    } = record
    else {
        panic!("Expected CatalogCheckpoint variant");
    };

    assert_eq!(checkpoint_lsn, 7000);
    assert_eq!(catalog_version, CatalogVersion::new(77));
    assert_eq!(visible_procedure_count, 9);
}

#[test]
fn catalog_version_projection_is_explicit() {
    let apply_record = apply_catalog_version_record(100, 42, 3, 99999);
    let checkpoint = checkpoint_record(7000, 77, 9);
    let create =
        create_procedure_record(1001, "my_procedure", ContractHash::test_vector(1), vec![]);

    assert_eq!(
        apply_record.catalog_version(),
        Some(CatalogVersion::new(42))
    );
    assert_eq!(checkpoint.catalog_version(), Some(CatalogVersion::new(77)));
    assert_eq!(create.catalog_version(), None);
}
