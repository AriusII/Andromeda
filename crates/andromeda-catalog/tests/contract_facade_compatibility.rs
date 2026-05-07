#![forbid(unsafe_code)]

use andromeda_catalog::{PolicyVersion, ProcedureContractRef, QualifiedName};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

#[test]
fn catalog_reexports_contract_crate_identities() {
    let name: andromeda_contract::QualifiedName =
        QualifiedName::parse("Inventory.ReserveStock").unwrap();
    assert_eq!(name.as_catalog_path(), "Inventory.ReserveStock");

    let reference: andromeda_contract::ProcedureContractRef = ProcedureContractRef {
        procedure_id: ProcedureId::new(1),
        contract_hash: ContractHash::test_vector(0xA1),
        catalog_version: CatalogVersion::new(7),
    };
    assert!(reference.validate().is_ok());

    let policy: andromeda_contract::PolicyVersion = PolicyVersion::new([0x5A; PolicyVersion::LEN]);
    assert!(!policy.is_zero());
}
