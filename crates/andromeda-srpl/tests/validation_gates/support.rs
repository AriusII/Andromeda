use andromeda_srpl::compile_narrow_procedure_signature;
use andromeda_srpl_ir::SrplProcedureIr;

pub(crate) fn compile_gate_source(srpl: &str, expectation: &str) -> SrplProcedureIr {
    compile_narrow_procedure_signature(srpl).expect(expectation)
}

pub(crate) fn assert_compile_path_does_not_panic(srpl: &str, desc: &str, category: &str) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = compile_narrow_procedure_signature(srpl);
    }));

    assert!(result.is_ok(), "Panic detected in {category}: {desc}");
}
