use super::*;

#[test]
fn registration_inserts_new_procedure_and_is_idempotent_for_same_entry() {
    let mut store = ProcedureStore::new();
    let e = entry(1, "Inventory.ReserveStock", 42);

    assert_eq!(
        store.register(e.clone()).unwrap(),
        ProcedureRegistration::Inserted
    );
    assert_eq!(store.len(), 1);
    assert_eq!(
        store.register(e.clone()).unwrap(),
        ProcedureRegistration::AlreadyRegistered
    );
    assert!(store.get(ProcedureId::new(1)).is_some());
    assert!(
        store
            .lookup_by_name(&QualifiedName::parse("Inventory.ReserveStock").unwrap())
            .is_some()
    );
}

#[test]
fn registration_rejects_contract_hash_divergence_for_same_id() {
    let mut store = ProcedureStore::new();
    store
        .register(entry(1, "Inventory.ReserveStock", 42))
        .unwrap();
    let err = store
        .register(entry(1, "Inventory.ReserveStock", 43))
        .unwrap_err();
    assert_eq!(err.kind(), andromeda_error::AndromedaErrorKind::Contract);
    assert!(err.message().contains("contract hash divergence"));
}

#[test]
fn registration_rejects_binding_divergence_for_same_id() {
    let mut store = ProcedureStore::new();
    let registered = entry(1, "Inventory.ReserveStock", 42);
    store.register(registered.clone()).unwrap();

    let mut drifted_stats = registered.clone();
    drifted_stats.binding.stats_version = StatsVersion::new(2);
    let err = store.register(drifted_stats).unwrap_err();
    assert_eq!(err.kind(), andromeda_error::AndromedaErrorKind::Contract);
    assert!(err.message().contains("binding divergence"));

    let mut drifted_policy = registered;
    drifted_policy.binding.policy_version = PolicyVersion::new([2; PolicyVersion::LEN]);
    let err = store.register(drifted_policy).unwrap_err();
    assert_eq!(err.kind(), andromeda_error::AndromedaErrorKind::Contract);
    assert!(err.message().contains("binding divergence"));
}

#[test]
fn registration_rejects_qualified_name_collision_across_ids() {
    let mut store = ProcedureStore::new();
    store
        .register(entry(1, "Inventory.ReserveStock", 42))
        .unwrap();
    let err = store
        .register(entry(2, "Inventory.ReserveStock", 99))
        .unwrap_err();
    assert_eq!(err.kind(), andromeda_error::AndromedaErrorKind::Catalog);
    assert!(err.message().contains("qualified name collision"));
}

#[test]
fn registration_rejects_zero_procedure_id() {
    let mut store = ProcedureStore::new();
    let bad = entry(0, "Inventory.ReserveStock", 42);
    let err = store.register(bad).unwrap_err();
    assert_eq!(err.kind(), andromeda_error::AndromedaErrorKind::Catalog);
}
