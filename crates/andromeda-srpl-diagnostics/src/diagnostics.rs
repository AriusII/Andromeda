pub use crate::source_location::SourceSpan;

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
    pub const fn diagnostic_code(self) -> &'static str {
        match self {
            Self::UnboundedWhile => "SRPL-FORBID-001",
            Self::FreeRecursion => "SRPL-FORBID-002",
            Self::ExternalNetwork => "SRPL-FORBID-003",
            Self::ExternalFilesystem => "SRPL-FORBID-004",
            Self::NondeterministicRandom => "SRPL-FORBID-005",
            Self::DynamicTextSql => "SRPL-FORBID-006",
            Self::SelectStar => "SRPL-FORBID-007",
        }
    }

    pub const fn diagnostic_phase(self) -> DiagnosticPhase {
        match self {
            Self::DynamicTextSql | Self::SelectStar => DiagnosticPhase::Binding,
            _ => DiagnosticPhase::SemanticValidation,
        }
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::UnboundedWhile => {
                "SRPL-FORBID-001: unbounded while loops are forbidden in SRPL core"
            },
            Self::FreeRecursion => "SRPL-FORBID-002: free recursion is forbidden in SRPL core",
            Self::ExternalNetwork => {
                "SRPL-FORBID-003: external network access is forbidden in SRPL core"
            },
            Self::ExternalFilesystem => {
                "SRPL-FORBID-004: external filesystem access is forbidden in SRPL core"
            },
            Self::NondeterministicRandom => {
                "SRPL-FORBID-005: nondeterministic random sources are forbidden in SRPL core"
            },
            Self::DynamicTextSql => "SRPL-FORBID-006: dynamic text SQL is forbidden in SRPL core",
            Self::SelectStar => "SRPL-FORBID-007: select star is forbidden in SRPL core",
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
