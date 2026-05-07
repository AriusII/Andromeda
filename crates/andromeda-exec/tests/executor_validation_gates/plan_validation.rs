use super::support::*;

#[test]
fn gate_exec_08_plan_validation_interface_available() {
    let result = SrplProcedureDispatcher::validate_plan(&valid_plan());

    assert!(result.is_ok(), "valid deterministic SRPL plan should pass");
}
