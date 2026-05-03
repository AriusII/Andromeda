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

impl SourceSpan {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub const fn is_valid(self) -> bool {
        self.start <= self.end
    }

    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplDiagnostic {
    pub phase: DiagnosticPhase,
    pub location: Option<SourceSpan>,
    pub message: String,
}

impl SrplDiagnostic {
    pub fn new(
        phase: DiagnosticPhase,
        location: Option<SourceSpan>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            phase,
            location,
            message: message.into(),
        }
    }

    pub fn forbidden_construct(hit: ForbiddenConstructHit) -> Self {
        Self::new(
            hit.construct.diagnostic_phase(),
            Some(hit.span),
            hit.construct.message(),
        )
    }
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

impl ForbiddenConstruct {
    pub const fn diagnostic_phase(self) -> DiagnosticPhase {
        match self {
            Self::DynamicTextSql | Self::SelectStar => DiagnosticPhase::Binding,
            _ => DiagnosticPhase::SemanticValidation,
        }
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::UnboundedWhile => "unbounded while loops are forbidden in SRPL core",
            Self::FreeRecursion => "free recursion is forbidden in SRPL core",
            Self::ExternalNetwork => "external network access is forbidden in SRPL core",
            Self::ExternalFilesystem => "external filesystem access is forbidden in SRPL core",
            Self::NondeterministicRandom => {
                "nondeterministic random sources are forbidden in SRPL core"
            }
            Self::DynamicTextSql => "dynamic text SQL is forbidden in SRPL core",
            Self::SelectStar => "select star is forbidden in SRPL core",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForbiddenConstructHit {
    pub construct: ForbiddenConstruct,
    pub span: SourceSpan,
}

impl ForbiddenConstructHit {
    pub const fn new(construct: ForbiddenConstruct, span: SourceSpan) -> Self {
        Self { construct, span }
    }

    pub fn diagnostic(self) -> SrplDiagnostic {
        SrplDiagnostic::forbidden_construct(self)
    }
}
