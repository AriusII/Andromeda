use andromeda_catalog::QualifiedName;
use andromeda_core::TypeDescriptor;

use crate::{Cardinality, SourceSpan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: SourceSpan,
}

impl<T> Spanned<T> {
    pub const fn new(value: T, span: SourceSpan) -> Self {
        Self { value, span }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureAst {
    pub name: Spanned<QualifiedName>,
    pub parameters: Vec<FieldAst>,
    pub results: Vec<ResultStreamAst>,
    pub body: ProcedureBodyAst,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldAst {
    pub name: Spanned<String>,
    pub data_type: Spanned<TypeDescriptor>,
    pub ordinal: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultStreamAst {
    pub name: Spanned<String>,
    pub cardinality: Spanned<Cardinality>,
    pub columns: Vec<FieldAst>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureBodyAst {
    pub operations: Vec<BusinessOperationAst>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusinessOperationAst {
    pub ordinal: u32,
    pub kind: BusinessOperationKindAst,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusinessOperationKindAst {
    Read {
        source: Spanned<QualifiedName>,
        binding: Spanned<String>,
        cardinality: Spanned<Cardinality>,
    },
    Assert {
        predicate: Spanned<String>,
        failure_code: Spanned<String>,
    },
    Update {
        target: Spanned<QualifiedName>,
        mutation: Spanned<String>,
    },
    Emit {
        stream: Spanned<String>,
        values: Vec<Spanned<String>>,
    },
    Raise {
        code: Spanned<String>,
    },
}
