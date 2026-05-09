#![forbid(unsafe_code)]

use andromeda_srpl_execution_adapter::SrplExecutionFailure;
use andromeda_srpl_interpreter::SrplIrInterpreter;
use andromeda_srpl_test_fixtures::{FakeSrplAdapter, srpl_happy_path_plan};

#[test]
fn owner_direct_interpreter_executes_bound_plan_without_srpl_facade() {
    let plan = srpl_happy_path_plan();
    let mut adapter = FakeSrplAdapter::passing();

    let report = SrplIrInterpreter::execute(&plan, &mut adapter)
        .expect("interpreter owner must execute executable plans directly");

    assert_eq!(
        adapter.events,
        vec!["read:0", "assert:1", "update:2", "emit:3"]
    );
    assert_eq!(report.operations_executed, 4);
    assert_eq!(report.reads, 1);
    assert_eq!(report.assertions, 1);
    assert_eq!(report.updates, 1);
    assert_eq!(report.emits, 1);
}

#[test]
fn owner_direct_interpreter_stops_after_update_cardinality_failure() {
    let plan = srpl_happy_path_plan();
    let mut adapter = FakeSrplAdapter {
        affected_rows: 0,
        ..FakeSrplAdapter::passing()
    };

    let error = SrplIrInterpreter::execute(&plan, &mut adapter)
        .expect_err("update cardinality mismatch must stop execution before emit");

    assert!(matches!(
        error,
        SrplExecutionFailure::CardinalityViolation { actual_rows: 0, .. }
    ));
    assert_eq!(adapter.events, vec!["read:0", "assert:1", "update:2"]);
}
