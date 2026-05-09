use andromeda_contract::ObjectKind;
use andromeda_srpl_cardinality::Cardinality;
use andromeda_srpl_ir::BoundSrplOperationPlan;
use andromeda_srpl_test_fixtures::{
    FakeSrplAdapter, srpl_happy_path_plan, srpl_procedure_object, srpl_procedure_ref,
    srpl_stock_object,
};
use andromeda_types::{CatalogVersion, ProcedureId};

#[test]
fn owner_fixtures_expose_valid_catalog_and_contract_refs() {
    let procedure_ref = srpl_procedure_ref();
    let procedure_object = srpl_procedure_object();
    let stock_object = srpl_stock_object();

    assert_eq!(procedure_ref.procedure_id, ProcedureId::new(7));
    assert_eq!(procedure_ref.catalog_version, CatalogVersion::new(3));
    assert_eq!(procedure_object.kind, ObjectKind::Procedure);
    assert_eq!(stock_object.kind, ObjectKind::Table);
}

#[test]
fn owner_fixtures_build_valid_happy_path_plan() {
    let plan = srpl_happy_path_plan();

    plan.validate().unwrap();
    assert_eq!(plan.body.operations.len(), 4);
    match &plan.body.operations[0] {
        BoundSrplOperationPlan::ReadTable { cardinality, .. } => {
            assert_eq!(*cardinality, Cardinality::One);
        },
        other => panic!("first happy-path operation must read stock, got {other:?}"),
    }
}

#[test]
fn owner_fake_adapter_has_deterministic_passing_defaults() {
    let adapter = FakeSrplAdapter::passing();

    assert!(adapter.assert_passes);
    assert_eq!(adapter.read_rows, 1);
    assert_eq!(adapter.affected_rows, 1);
    assert_eq!(adapter.emitted_rows, 1);
    assert!(adapter.events.is_empty());
}
