#[allow(dead_code)]
#[path = "common.rs"]
mod common;

use andromeda_catalog::{CatalogLifecycleAction, CatalogLifecycleTarget, CatalogMutationOperation};

use common::*;

#[test]
fn catalog_diff_deprecate_lifecycle_produces_deprecated_object_delta() {
    let base_version = CatalogVersion::new(4);
    let next_version = CatalogVersion::new(5);
    let target = object_ref(
        801,
        "test.DeprecatedProcedure",
        ObjectKind::Procedure,
        base_version,
    );
    let batch = definition_batch(
        base_version,
        vec![DefinitionOperation::Deprecate(CatalogLifecycleTarget {
            object: target.clone(),
        })],
    );

    let plan = batch.dry_run().expect("deprecate batch must plan");

    assert_eq!(plan.created_objects.len(), 0);
    assert_eq!(plan.deprecated_objects.len(), 1);
    let deprecated = &plan.deprecated_objects[0];
    assert_eq!(deprecated.object_id, target.object_id);
    assert_eq!(deprecated.name, target.name);
    assert_eq!(deprecated.kind, ObjectKind::Procedure);
    assert_eq!(deprecated.action, CatalogLifecycleAction::Deprecate);
    assert_eq!(deprecated.planned_version, next_version);

    assert_eq!(plan.mutation_plan.deltas.len(), 1);
    let delta = &plan.mutation_plan.deltas[0];
    assert_eq!(delta.operation_index, 0);
    assert_eq!(delta.planned_version, next_version);
    assert_eq!(delta.object(), &target);
    assert!(
        delta.definition().is_none(),
        "deprecation lifecycle deltas must not carry a replacement definition"
    );

    let CatalogMutationOperation::DeprecateObject {
        target: planned_target,
    } = &delta.operation
    else {
        panic!("expected deprecate-object mutation delta");
    };
    assert_eq!(planned_target.object, target);
    assert_eq!(plan.mutation_plan.record_count(), 3);
}

#[test]
fn catalog_diff_rejects_implicit_create_and_deprecate_of_same_object() {
    let base_version = CatalogVersion::new(6);
    let next_version = CatalogVersion::new(7);
    let target = object_ref(
        802,
        "test.ReplaceWithoutExplicitAlter",
        ObjectKind::Procedure,
        base_version,
    );
    let batch = definition_batch(
        base_version,
        vec![
            create_procedure_operation(802, "test.ReplaceWithoutExplicitAlter", next_version),
            DefinitionOperation::Deprecate(CatalogLifecycleTarget { object: target }),
        ],
    );

    let error = batch
        .dry_run()
        .expect_err("implicit replace must be rejected without explicit compatibility evidence");
    assert!(
        error.message().contains("same lifecycle object id twice"),
        "unexpected error: {}",
        error.message()
    );
}
