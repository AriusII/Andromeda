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
