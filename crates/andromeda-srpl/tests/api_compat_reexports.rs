#![forbid(unsafe_code)]

use andromeda_optimizer::srpl::{OptimizerDiagnostic, OptimizerPipelineConfig};
use andromeda_srpl::compile_narrow_procedure_signature;
use andromeda_srpl_ast::{ProcedureAst, Spanned};
use andromeda_srpl_cardinality::Cardinality;
use andromeda_srpl_diagnostics::{
    DiagnosticPhase, ForbiddenConstruct, ForbiddenConstructHit, SourceSpan, SrplDiagnostic,
    SrplSource,
};
use andromeda_srpl_execution_adapter::{
    SrplAssertResult, SrplBindingEnvironment, SrplEmitResult, SrplExecutionFailure, SrplReadResult,
    SrplUpdateResult,
};
use andromeda_srpl_ir::{
    BoundSrplBodyPlan, BoundSrplOperationPlan, ExecutableProcedurePlan, ProcedureSignature,
    ResultContract, SrplPredicateIr, SrplProcedureIr, SrplValueIr,
};
use andromeda_srpl_lexer::{Token, TokenKind, lex};
use andromeda_srpl_parser::parse_procedure_signature;

fn assert_type<T>() {}
fn assert_same_type<T>(_value: T) {}

#[test]
fn srpl_tests_import_owner_crate_types_directly() {
    assert_type::<Cardinality>();
    assert_type::<DiagnosticPhase>();
    assert_type::<ForbiddenConstruct>();
    assert_type::<ForbiddenConstructHit>();
    assert_type::<ProcedureAst>();
    assert_type::<SourceSpan>();
    assert_type::<SrplDiagnostic>();
    assert_type::<SrplSource<'static>>();
    assert_type::<SrplSource<'static>>();

    assert_type::<SrplAssertResult>();
    assert_type::<SrplEmitResult>();
    assert_type::<SrplExecutionFailure>();
    assert_type::<SrplReadResult<()>>();
    assert_type::<SrplUpdateResult>();
    assert_type::<&'static dyn SrplBindingEnvironment>();

    assert_type::<OptimizerDiagnostic>();
    assert_type::<OptimizerPipelineConfig>();

    assert_type::<Token>();
    assert_type::<TokenKind>();
    assert_type::<BoundSrplBodyPlan>();
    assert_type::<BoundSrplOperationPlan>();
    assert_type::<ExecutableProcedurePlan>();
    assert_type::<ProcedureSignature>();
    assert_type::<ResultContract>();
    assert_type::<SrplPredicateIr>();
    assert_type::<SrplProcedureIr>();
    assert_type::<SrplValueIr>();
    assert_type::<SourceSpan>();
}

#[test]
fn owner_crate_types_are_not_wrapped_by_srpl_orchestration() {
    assert_same_type::<Cardinality>(Cardinality::One);

    let owner_span = SourceSpan::new(3, 9);
    assert_eq!(owner_span, SourceSpan::new(3, 9));

    let owner_source = SrplSource::new("select *");
    let module_source: SrplSource<'_> = SrplSource::new("select *");
    assert!(
        owner_source
            .forbidden_constructs()
            .contains(&ForbiddenConstruct::SelectStar)
    );
    assert!(
        module_source
            .forbidden_constructs()
            .contains(&ForbiddenConstruct::SelectStar)
    );

    let owner_diag = SrplDiagnostic::new(
        DiagnosticPhase::Binding,
        Some(owner_span),
        "owner diagnostic",
    );
    assert_eq!(owner_diag.phase, DiagnosticPhase::Binding);

    let owner_spanned = Spanned::new(Cardinality::Many, owner_span);
    assert_eq!(owner_spanned.value, Cardinality::Many);
}

#[test]
fn srpl_orchestration_keeps_compiler_entrypoint_only() {
    let source = "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool);";

    let tokens = lex(source).expect("lexer owner crate must stay callable");
    assert!(
        tokens
            .iter()
            .any(|token| token.kind == TokenKind::Procedure)
    );

    let ast = parse_procedure_signature(source).expect("parser owner crate must stay callable");
    assert_eq!(ast.name.value.as_catalog_path(), "Inventory.ReserveStock");

    let ir = compile_narrow_procedure_signature(source)
        .expect("compiler orchestration must still return the public SRPL IR model");
    assert_eq!(ir.result_streams[0].cardinality, Cardinality::One);
}
