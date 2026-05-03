#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticPhase {
    Lexing,
    Parsing,
    Binding,
    SemanticValidation,
    IrLowering,
    PlanCandidateGeneration,
    RuntimeBinding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplDiagnostic {
    pub phase: DiagnosticPhase,
    pub location: Option<SourceSpan>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForbiddenConstruct {
    UnboundedWhile,
    FreeRecursion,
    ExternalNetwork,
    ExternalFilesystem,
    NondeterministicRandom,
    DynamicTextSql,
    SelectStar,
}
