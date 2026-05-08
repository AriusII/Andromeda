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

#[test]
fn catalog_facade_preserves_contract_and_object_type_identities() {
    let _: fn(
        &andromeda_catalog::ProcedureContract,
        &andromeda_catalog::ProcedureContract,
    ) -> andromeda_catalog::ContractCompatibilityDiagnostic =
        andromeda_catalog::diagnose_procedure_contract_compatibility;
    let _: fn(
        &andromeda_contract::ProcedureContract,
        &andromeda_contract::ProcedureContract,
    ) -> andromeda_contract::ContractCompatibilityDiagnostic =
        andromeda_catalog::diagnose_procedure_contract_compatibility;

    let _: Option<andromeda_contract::ProcedureContract> =
        Option::<andromeda_catalog::ProcedureContract>::None;
    let _: Option<andromeda_catalog::ProcedureContract> =
        Option::<andromeda_contract::ProcedureContract>::None;

    let _: Option<andromeda_contract::ProcedureContractCandidate> =
        Option::<andromeda_catalog::ProcedureContractCandidate>::None;
    let _: Option<andromeda_catalog::ProcedureContractCandidate> =
        Option::<andromeda_contract::ProcedureContractCandidate>::None;

    let _: Option<andromeda_contract::ProcedureContractBinding> =
        Option::<andromeda_catalog::ProcedureContractBinding>::None;
    let _: Option<andromeda_catalog::ProcedureContractBinding> =
        Option::<andromeda_contract::ProcedureContractBinding>::None;

    let _: Option<andromeda_contract::ContractCompatibilityDiagnostic> =
        Option::<andromeda_catalog::ContractCompatibilityDiagnostic>::None;
    let _: Option<andromeda_catalog::ContractCompatibilityDiagnostic> =
        Option::<andromeda_contract::ContractCompatibilityDiagnostic>::None;

    let _: Option<andromeda_contract::ResultStreamContract> =
        Option::<andromeda_catalog::ResultStreamContract>::None;
    let _: Option<andromeda_catalog::ResultStreamContract> =
        Option::<andromeda_contract::ResultStreamContract>::None;

    let _: Option<andromeda_contract::ResultStreamCardinality> =
        Option::<andromeda_catalog::ResultStreamCardinality>::None;
    let _: Option<andromeda_catalog::ResultStreamCardinality> =
        Option::<andromeda_contract::ResultStreamCardinality>::None;

    let _: Option<andromeda_contract::CatalogDefinition> =
        Option::<andromeda_catalog::CatalogDefinition>::None;
    let _: Option<andromeda_catalog::CatalogDefinition> =
        Option::<andromeda_contract::CatalogDefinition>::None;

    let _: Option<andromeda_contract::StructuredObjectDefinition> =
        Option::<andromeda_catalog::StructuredObjectDefinition>::None;
    let _: Option<andromeda_catalog::StructuredObjectDefinition> =
        Option::<andromeda_contract::StructuredObjectDefinition>::None;

    let _: Option<andromeda_contract::TableDefinition> =
        Option::<andromeda_catalog::TableDefinition>::None;
    let _: Option<andromeda_catalog::TableDefinition> =
        Option::<andromeda_contract::TableDefinition>::None;

    let _: Option<andromeda_contract::CatalogObjectBinding> =
        Option::<andromeda_catalog::CatalogObjectBinding>::None;
    let _: Option<andromeda_catalog::CatalogObjectBinding> =
        Option::<andromeda_contract::CatalogObjectBinding>::None;

    let _: Option<andromeda_contract::CatalogDependency> =
        Option::<andromeda_catalog::CatalogDependency>::None;
    let _: Option<andromeda_catalog::CatalogDependency> =
        Option::<andromeda_contract::CatalogDependency>::None;

    let _: Option<andromeda_contract::QualifiedName> =
        Option::<andromeda_catalog::QualifiedName>::None;
    let _: Option<andromeda_catalog::QualifiedName> =
        Option::<andromeda_contract::QualifiedName>::None;
}
