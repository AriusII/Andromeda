#![forbid(unsafe_code)]

use andromeda_contract::{CatalogObjectRef, ObjectKind, ProcedureContractRef, QualifiedName};
use andromeda_error::AndromedaErrorKind;
use andromeda_srpl_execution_adapter::{
    SrplBindingEnvironment, SrplBoundValue, SrplEmitRequest, SrplReadRequest, SrplRowBound,
};
use andromeda_srpl_ir::{Cardinality, SrplEmitValueIr, SrplValueIr};
use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash, ProcedureId};

#[test]
fn owner_direct_adapter_rejects_cardinality_bound_mismatch() {
    let error = SrplReadRequest::new(
        context(0),
        object(ObjectKind::Table),
        Cardinality::One,
        SrplRowBound::exact(2).unwrap(),
        Vec::new(),
    )
    .expect_err("adapter owner must reject row bounds incompatible with cardinality");

    assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
    assert!(error.message().contains("cardinality"));
}

#[test]
fn owner_direct_adapter_rejects_sql_like_freeform_symbols() {
    let error = SrplEmitRequest::new(
        context(1),
        "select",
        Cardinality::One,
        SrplRowBound::exact(1).unwrap(),
        vec![SrplEmitValueIr {
            column: "Reserved".to_string(),
            value: SrplValueIr::bool(true),
        }],
    )
    .expect_err("adapter owner must reject SQL-like stream symbols");

    assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
    assert!(error.message().contains("SQL-like"));
}

#[test]
fn owner_direct_binding_environment_rejects_missing_and_implicit_null_values() {
    let environment = OwnerDirectEnvironment;

    let missing = environment
        .get_required_input("Missing")
        .expect_err("missing inputs must not become implicit nulls");
    let null = environment
        .get_required_input("Nullable")
        .expect_err("explicit nulls are rejected by required input access");
    let value = environment
        .get_required_field_from_binding("Stock", 0, "AvailableQuantity")
        .expect("bound fields should pass required access");

    assert_eq!(missing.kind(), AndromedaErrorKind::Srpl);
    assert_eq!(null.kind(), AndromedaErrorKind::Srpl);
    assert_eq!(value, SrplBoundValue::Integer(10));
}

struct OwnerDirectEnvironment;

impl SrplBindingEnvironment for OwnerDirectEnvironment {
    fn get_input(&self, name: &str) -> Option<SrplBoundValue> {
        match name {
            "Nullable" => Some(SrplBoundValue::Null),
            _ => None,
        }
    }

    fn get_field_from_binding(
        &self,
        binding: &str,
        row_index: usize,
        field: &str,
    ) -> Option<SrplBoundValue> {
        match (binding, row_index, field) {
            ("Stock", 0, "AvailableQuantity") => Some(SrplBoundValue::Integer(10)),
            _ => None,
        }
    }

    fn binding_row_count(&self, binding: &str) -> Option<usize> {
        (binding == "Stock").then_some(1)
    }
}

fn context(ordinal: u32) -> andromeda_srpl_execution_adapter::SrplOperationContext {
    andromeda_srpl_execution_adapter::SrplOperationContext::new(
        ProcedureContractRef {
            procedure_id: ProcedureId::new(7),
            contract_hash: ContractHash::test_vector(0xA7),
            catalog_version: CatalogVersion::new(3),
        },
        ordinal,
    )
    .unwrap()
}

fn object(kind: ObjectKind) -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(17),
        name: QualifiedName::parse("Inventory.ProductStock").unwrap(),
        kind,
        catalog_version: CatalogVersion::new(3),
    }
}
