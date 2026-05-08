#![forbid(unsafe_code)]

use andromeda_srpl::{
    Cardinality, DiagnosticPhase, ForbiddenConstruct, ForbiddenConstructHit, ProcedureAst,
    SourceSpan, Spanned, SrplDiagnostic, SrplSource, compile_narrow_procedure_signature,
    definition_batch_bridge::{
        SrplDefinitionBatchDiagnostic, SrplDefinitionBatchDryRunReport,
        SrplDefinitionBatchDryRunRequest,
    },
    execution_adapter::{
        SrplAssertResult, SrplBindingEnvironment, SrplEmitResult, SrplExecutionFailure,
        SrplReadResult, SrplUpdateResult,
    },
    interpreter::{SrplInterpreterReport, SrplIrInterpreter},
    optimizer::{OptimizerDiagnostic, OptimizerPipelineConfig},
    parse_procedure_signature,
    procedure_compiler::{Token, TokenKind, lex},
    procedure_model::{
        BoundSrplBodyPlan, BoundSrplOperationPlan, BusinessOperationKindAst,
        ExecutableProcedurePlan, ProcedureSignature, ResultContract, SrplPredicateIr,
        SrplProcedureIr, SrplValueIr,
    },
    procedure_resolver::{
        ProcedureResolveError, ProcedureResolveRequest, ProcedureResolveResponse,
        ProcedureResolveTarget, SrplProcedureManifest,
    },
    source_location,
};

fn assert_type<T>() {}
fn assert_same_type<T>(_value: T) {}

#[test]
fn srpl_facade_preserves_historical_public_imports() {
    assert_type::<Cardinality>();
    assert_type::<DiagnosticPhase>();
    assert_type::<ForbiddenConstruct>();
    assert_type::<ForbiddenConstructHit>();
    assert_type::<ProcedureAst>();
    assert_type::<SourceSpan>();
    assert_type::<SrplDiagnostic>();
    assert_type::<SrplSource<'static>>();
    assert_type::<source_location::SrplSource<'static>>();

    assert_type::<SrplDefinitionBatchDiagnostic>();
    assert_type::<SrplDefinitionBatchDryRunReport>();
    assert_type::<SrplDefinitionBatchDryRunRequest>();

    assert_type::<SrplAssertResult>();
    assert_type::<SrplEmitResult>();
    assert_type::<SrplExecutionFailure>();
    assert_type::<SrplReadResult<()>>();
    assert_type::<SrplUpdateResult>();
    assert_type::<&'static dyn SrplBindingEnvironment>();
    assert_type::<SrplInterpreterReport>();
    assert_type::<SrplIrInterpreter>();

    assert_type::<OptimizerDiagnostic>();
    assert_type::<OptimizerPipelineConfig>();

    assert_type::<Token>();
    assert_type::<TokenKind>();
    assert_type::<BoundSrplBodyPlan>();
    assert_type::<BoundSrplOperationPlan>();
    assert_type::<BusinessOperationKindAst>();
    assert_type::<ExecutableProcedurePlan>();
    assert_type::<ProcedureSignature>();
    assert_type::<ResultContract>();
    assert_type::<SrplPredicateIr>();
    assert_type::<SrplProcedureIr>();
    assert_type::<SrplValueIr>();

    assert_type::<ProcedureResolveError>();
    assert_type::<ProcedureResolveRequest>();
    assert_type::<ProcedureResolveResponse>();
    assert_type::<ProcedureResolveTarget>();
    assert_type::<SrplProcedureManifest>();
    assert_type::<source_location::SourceSpan>();
}

#[test]
fn srpl_facade_reexports_owner_crate_types_without_wrapping() {
    assert_same_type::<andromeda_srpl_cardinality::Cardinality>(Cardinality::One);
    assert_same_type::<Cardinality>(andromeda_srpl_cardinality::Cardinality::One);

    let owner_span = andromeda_srpl_diagnostics::SourceSpan::new(3, 9);
    let facade_span: SourceSpan = owner_span;
    assert_eq!(facade_span, SourceSpan::new(3, 9));

    let owner_source = andromeda_srpl_diagnostics::SrplSource::new("select *");
    let facade_source: SrplSource<'_> = owner_source;
    let facade_module_source: source_location::SrplSource<'_> =
        andromeda_srpl_diagnostics::SrplSource::new("select *");
    assert!(
        facade_source
            .forbidden_constructs()
            .contains(&ForbiddenConstruct::SelectStar)
    );
    assert!(
        facade_module_source
            .forbidden_constructs()
            .contains(&ForbiddenConstruct::SelectStar)
    );

    let owner_diag = andromeda_srpl_diagnostics::SrplDiagnostic::new(
        DiagnosticPhase::Binding,
        Some(facade_span),
        "owner diagnostic",
    );
    let facade_diag: SrplDiagnostic = owner_diag;
    assert_eq!(facade_diag.phase, DiagnosticPhase::Binding);

    let owner_spanned = andromeda_srpl_ast::Spanned::new(Cardinality::Many, facade_span);
    let facade_spanned: Spanned<Cardinality> = owner_spanned;
    assert_eq!(facade_spanned.value, Cardinality::Many);
}

#[test]
fn srpl_facade_entrypoints_still_use_the_historical_paths() {
    let source = "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool);";

    let tokens = lex(source).expect("lexer facade must stay callable");
    assert!(
        tokens
            .iter()
            .any(|token| token.kind == TokenKind::Procedure)
    );

    let ast = parse_procedure_signature(source).expect("parser facade must stay callable");
    assert_eq!(ast.name.value.as_catalog_path(), "Inventory.ReserveStock");

    let ir = compile_narrow_procedure_signature(source)
        .expect("compiler facade must still return the public SRPL IR model");
    assert_eq!(ir.result_streams[0].cardinality, Cardinality::One);
}
